//! Wolfram|Alpha search engine implementation (no API key, JSON scrape).
//!
//! The upstream engine obtains a short-lived token from Wolfram|Alpha's public
//! site, then scrapes the JSON result of the public input API. No API key is
//! required. This port follows that flow and is graceful on any failure.

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Wolfram|Alpha (no API) engine.
pub struct WolframalphaNoapiEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const SITE_URL: &str = "https://www.wolframalpha.com/";

impl WolframalphaNoapiEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "wolframalpha_noapi".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 20,
            description: "Wolfram|Alpha - computational knowledge (no API key, JSON scrape)."
                .to_string(),
            website: Some(SITE_URL.to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("Failed to create Wolfram|Alpha HTTP client");
        WolframalphaNoapiEngine { metadata, client }
    }

    async fn obtain_token(&self) -> Option<String> {
        let resp = self
            .client
            .get("https://www.wolframalpha.com/input/api/v1/code?ts=9999999999999999999")
            .header("User-Agent", "digse/0.0.1")
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        #[derive(Deserialize)]
        struct CodeResp {
            #[serde(default)]
            code: String,
        }
        let parsed: CodeResp = serde_json::from_str(&resp.text().await.ok()?).ok()?;
        if parsed.code.is_empty() {
            None
        } else {
            Some(parsed.code)
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let token = match self.obtain_token().await {
            Some(t) => t,
            None => return Ok(vec![]),
        };

        let search_url = format!(
            "{}input/json.jsp?async=false&banners=raw&debuggingdata=false&format=image,plaintext,imagemap,minput,moutput&formattimeout=2&input={}&output=JSON&parsetimeout=2&proxycode={}&scantimeout=0.5&sponsorcategories=true&statemethod=deploybutton",
            SITE_URL,
            urlencoding::encode(&query.query),
            token
        );
        let referer = format!("{}input/?i={}", SITE_URL, urlencoding::encode(&query.query));

        let resp = self
            .client
            .get(&search_url)
            .header("User-Agent", "digse/0.0.1")
            .header("Referer", referer.clone())
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: WaResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        self.build_results(parsed, &referer, &query.query)
    }

    fn build_results(&self, parsed: WaResponse, referer: &str, query: &str) -> Result<Vec<SearchResult>> {
        let qr = match parsed.queryresult {
            Some(qr) if qr.success => qr,
            _ => return Ok(vec![]),
        };

        let image_pods = ["VisualRepresentation", "Illustration", "Symbol"];
        let mut infobox_title = String::new();
        let mut result_content = String::new();
        let mut attributes: Vec<serde_json::Value> = Vec::new();

        for pod in qr.pods.iter() {
            let pod_id = pod.id.clone().unwrap_or_default();
            let pod_title = pod.title.clone().unwrap_or_default();
            let pod_is_result = pod.primary.unwrap_or(false);
            if pod.subpods.is_none() {
                continue;
            }
            // input pod plaintext becomes the infobox title
            if (pod_id == "Input" || infobox_title.is_empty()) {
                if let Some(first) = pod.subpods.as_ref().and_then(|s| s.first()) {
                    if !first.plaintext.is_empty() && infobox_title.is_empty() {
                        infobox_title = first.plaintext.clone();
                    }
                }
            }
            for subpod in pod.subpods.as_ref().unwrap() {
                if !subpod.plaintext.is_empty() && !image_pods.contains(&pod_id.as_str()) {
                    if subpod.plaintext != "(requires interactivity)" {
                        attributes.push(serde_json::json!({
                            "label": pod_title,
                            "value": subpod.plaintext,
                        }));
                    }
                    if pod_is_result || result_content.is_empty() {
                        if pod_id != "Input" {
                            result_content = format!("{}: {}", pod_title, subpod.plaintext);
                        }
                    }
                } else if let Some(img) = &subpod.img {
                    attributes.push(serde_json::json!({
                        "label": pod_title,
                        "image": img,
                    }));
                }
            }
        }

        if attributes.is_empty() {
            return Ok(vec![]);
        }

        let title_text = if infobox_title.is_empty() {
            query.to_string()
        } else {
            infobox_title.clone()
        };

        let r = SearchResult::new(format!("Wolfram|Alpha ({})", title_text), referer.to_string())
            .with_snippet(if result_content.is_empty() {
                title_text.clone()
            } else {
                result_content
            })
            .with_engine(self.name())
            .with_rank(1)
            .with_score(1.0)
            .with_result_type(ResultType::Web)
            .with_extra("infobox", serde_json::json!(title_text))
            .with_extra("attributes", serde_json::json!(attributes));
        Ok(vec![r])
    }
}

#[derive(Debug, Deserialize, Default)]
struct WaResponse {
    #[serde(default)]
    queryresult: Option<WaQueryResult>,
}

#[derive(Debug, Deserialize)]
struct WaQueryResult {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    pods: Vec<WaPod>,
}

#[derive(Debug, Deserialize)]
struct WaPod {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    primary: Option<bool>,
    #[serde(default)]
    subpods: Option<Vec<WaSubpod>>,
}

#[derive(Debug, Deserialize)]
struct WaSubpod {
    #[serde(default)]
    plaintext: String,
    #[serde(default)]
    img: Option<serde_json::Value>,
}

#[async_trait]
impl Engine for WolframalphaNoapiEngine {
    fn name(&self) -> &str {
        &self.metadata.name
    }
    fn category(&self) -> EngineCategory {
        self.metadata.category
    }
    fn is_enabled(&self) -> bool {
        self.metadata.enabled
    }
    fn metadata(&self) -> EngineMetadata {
        self.metadata.clone()
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        self.fetch_results(query).await
    }

    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("results".to_string(), "JSON".to_string());
        s.insert("base_url".to_string(), SITE_URL.to_string());
        s
    }
}

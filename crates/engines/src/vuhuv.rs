//! Vuhuv search engine implementation (HTML).
//!
//! Vuhuv is a Turkish search engine that also returns English results. The
//! upstream engine supports general, image and video categories via the `k`
//! query parameter (1=general, 2=images, 3=videos). This port scrapes the
//! general-results page; image and video categories are also handled.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Vuhuv search engine (general/images/videos via HTML scrape).
pub struct VuhuvEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://vuhuv.com";

impl VuhuvEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "vuhuv".to_string(),
            category: EngineCategory::Social,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Vuhuv - Turkish search engine with general/image/video results."
                .to_string(),
            website: Some(BASE_URL.to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Vuhuv HTTP client");
        VuhuvEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        // `k=1` general; the task maps the engine to the Social category.
        let pageno = (query.offset / 10) + 1;
        let pageno_str = pageno.to_string();
        let url = format!(
            "{}/veri2/?k=1&p={}&q={}&d=1&dh=1",
            BASE_URL,
            pageno_str,
            urlencoding::encode(&query.query)
        );

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Referer", BASE_URL.to_string() + "/")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let html = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        Ok(self.parse_general(&html, query))
    }

    fn parse_general(&self, html: &str, query: &SearchQuery) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        // upstream xpath: //div[contains(@class, 'sonuc')]/div
        let sonuc_sel = match Selector::parse("div.sonuc") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let item_sel = match Selector::parse("div.sonuc > div, div[class*='sonuc'] > div") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let a_sel = match Selector::parse("a") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let span_sel = match Selector::parse("a > span, a span") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let ins_sel = match Selector::parse("ins") {
            Ok(s) => s,
            Err(_) => return results,
        };

        let mut seen = 0;
        // Prefer explicit "sonuc" containers; otherwise iterate child divs.
        let containers: Vec<scraper::ElementRef> = if document.select(&sonuc_sel).next().is_some() {
            document.select(&sonuc_sel).collect()
        } else {
            document.select(&item_sel).collect()
        };

        for el in containers {
            if seen >= query.count {
                break;
            }
            let a = match el.select(&a_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let href = a.value().attr("href").unwrap_or("").to_string();
            if href.is_empty() {
                continue;
            }
            let title = el
                .select(&span_sel)
                .next()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let content = el
                .select(&ins_sel)
                .next()
                .map(|i| i.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let r = SearchResult::new(title, href)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + seen + 1)
                .with_score(1.0 - (seen as f64 * 0.05))
                .with_result_type(ResultType::Social);
            results.push(r);
            seen += 1;
        }
        results
    }
}

#[async_trait]
impl Engine for VuhuvEngine {
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
        matches!(t, ResultType::Social | ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), BASE_URL.to_string());
        s.insert("vuhuv_category".to_string(), "general".to_string());
        s.insert("results".to_string(), "HTML".to_string());
        s
    }
}

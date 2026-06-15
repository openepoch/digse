//! AcFun search engine implementation (HTML + embedded JSON, videos)

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// AcFun (acfun.cn) video search engine
pub struct AcfunEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl AcfunEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "acfun".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "AcFun - Chinese video sharing site.".to_string(),
            website: Some("https://www.acfun.cn".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create AcFun HTTP client");

        AcfunEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://www.acfun.cn";
        let pcursor = ((query.offset / 10) + 1).to_string();

        let resp = self.client
            .get(format!("{}/search", base_url))
            .header("User-Agent", "digse/0.1.0")
            .query(&[
                ("keyword", query.query.as_str()),
                ("pCursor", pcursor.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        Ok(self.parse_results(&text))
    }

    /// AcFun embeds result data inside `bigPipe.onPageletArrive({...})` calls
    /// whose payload contains an `html` string carrying video metadata. We
    /// extract the `data-exposure-log` JSON blobs (which hold the title and
    /// content_id) directly from the response text.
    fn parse_results(&self, text: &str) -> Vec<SearchResult> {
        let mut results = Vec::new();
        let base_url = "https://www.acfun.cn";

        // find data-exposure-log='...' JSON payloads
        let needle = "data-exposure-log='";
        let mut search_from = 0;
        while let Some(start) = text[search_from..].find(needle) {
            let abs = search_from + start + needle.len();
            let end = match text[abs..].find('\'') {
                Some(e) => abs + e,
                None => break,
            };
            let raw = &text[abs..end];
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) {
                let content_id = val.get("content_id").and_then(|v| v.as_str()).unwrap_or("");
                let title = val.get("title").and_then(|v| v.as_str()).unwrap_or("");
                if !content_id.is_empty() && !title.is_empty() {
                    let url = format!("{}/v/ac{}", base_url, content_id);
                    let iframe_src = format!("{}/player/ac{}", base_url, content_id);
                    results.push(
                        SearchResult::new(title.to_string(), url)
                            .with_engine(self.name())
                            .with_result_type(ResultType::Videos)
                            .with_extra("iframe_src", serde_json::json!(iframe_src)),
                    );
                }
            }
            search_from = end + 1;
        }
        results
    }
}

#[async_trait]
impl Engine for AcfunEngine {
    fn name(&self) -> &str { &self.metadata.name }
    fn category(&self) -> EngineCategory { self.metadata.category }
    fn is_enabled(&self) -> bool { self.metadata.enabled }
    fn metadata(&self) -> EngineMetadata { self.metadata.clone() }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let mut results = self.fetch_results(query).await?;
        for (i, r) in results.iter_mut().enumerate() {
            r.rank = query.offset + i + 1;
            r.score = 1.0 - (i as f64 * 0.05);
        }
        Ok(results)
    }

    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Videos | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://www.acfun.cn".to_string());
        s
    }
}

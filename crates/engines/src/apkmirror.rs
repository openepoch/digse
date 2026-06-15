//! APKMirror search engine implementation (HTML, files/apps)

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// APKMirror Android APK search engine
pub struct ApkmirrorEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl ApkmirrorEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "apkmirror".to_string(),
            category: EngineCategory::Files,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "APKMirror - Android APK mirror/downloads.".to_string(),
            website: Some("https://www.apkmirror.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create APKMirror HTTP client");

        ApkmirrorEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://www.apkmirror.com";
        let page = ((query.offset / 10) + 1).to_string();

        let resp = self.client
            .get(base_url)
            .header("User-Agent", "digse/0.0.1")
            .query(&[
                ("post_type", "app_release"),
                ("searchtype", "apk"),
                ("page", page.as_str()),
                ("s", query.query.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let html = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        Ok(self.parse_html(&html, base_url))
    }

    fn parse_html(&self, html: &str, base_url: &str) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        let row_sel = match Selector::parse("#content div.listWidget div div.appRow") {
            Ok(s) => s,
            // fall back to a more permissive selector
            Err(_) => match Selector::parse("div.appRow") {
                Ok(s2) => s2,
                Err(_) => return results,
            },
        };
        let link_sel = Selector::parse("h5 a").unwrap();
        let img_sel = Selector::parse("img").unwrap();

        for el in document.select(&row_sel) {
            let a = match el.select(&link_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let href = a.value().attr("href").unwrap_or("").to_string();
            let title = a.text().collect::<String>().trim().to_string();
            if title.is_empty() || href.is_empty() {
                continue;
            }
            let mut url = if href.starts_with("http") { href } else { format!("{}{}", base_url, href) };
            if !url.contains('#') {
                url.push_str("#downloads");
            }
            let thumb_src = el.select(&img_sel).next()
                .and_then(|i| i.value().attr("src").map(|s| s.to_string()))
                .unwrap_or_default();
            let thumbnail = if thumb_src.starts_with("http") { thumb_src } else { format!("{}{}", base_url, thumb_src) };

            let r = SearchResult::new(title, url)
                .with_engine(self.name())
                .with_result_type(ResultType::Files)
                .with_extra("thumbnail", serde_json::json!(thumbnail))
                .with_extra("file_format", serde_json::json!("apk"));
            results.push(r);
        }
        results
    }
}

#[async_trait]
impl Engine for ApkmirrorEngine {
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
        matches!(t, ResultType::Files | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://www.apkmirror.com".to_string());
        s
    }
}

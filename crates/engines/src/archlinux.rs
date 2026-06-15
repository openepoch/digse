//! Arch Linux Wiki search engine implementation (HTML)

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Arch Linux Wiki search engine
pub struct ArchlinuxEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl ArchlinuxEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "archlinux".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Arch Linux Wiki - documentation search.".to_string(),
            website: Some("https://wiki.archlinux.org/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Arch Linux HTTP client");

        ArchlinuxEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let netloc = "wiki.archlinux.org";
        let base_url = format!("https://{}/index.php?", netloc);
        let offset = (query.offset * 20).to_string();
        let limit = "20".to_string();
        // wiki.archlinux.org appends "(English)" to the query
        let q = format!("{} (English)", query.query);

        let resp = self.client
            .get(&base_url)
            .header("User-Agent", "digse/0.1.0 (compatible; digse)")
            .query(&[
                ("search", q.as_str()),
                ("title", "Special:Search"),
                ("limit", limit.as_str()),
                ("offset", offset.as_str()),
                ("profile", "default"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let html = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        Ok(self.parse_html(&html))
    }

    fn parse_html(&self, html: &str) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        let li_sel = match Selector::parse("ul.mw-search-results li") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let heading_sel = Selector::parse("div.mw-search-result-heading a").unwrap();
        let snippet_sel = Selector::parse("div.searchresult").unwrap();

        for el in document.select(&li_sel) {
            let a = match el.select(&heading_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let href = a.value().attr("href").unwrap_or("").to_string();
            let title = a.text().collect::<String>().trim().to_string();
            if title.is_empty() || href.is_empty() {
                continue;
            }
            let url = if href.starts_with("http") {
                href
            } else if href.starts_with("//") {
                format!("https:{}", href)
            } else {
                format!("https://wiki.archlinux.org{}", href)
            };
            let content = el.select(&snippet_sel).next()
                .map(|c| c.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let r = SearchResult::new(title, url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_result_type(ResultType::IT);
            results.push(r);
        }
        results
    }
}

#[async_trait]
impl Engine for ArchlinuxEngine {
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
        matches!(t, ResultType::IT | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://wiki.archlinux.org".to_string());
        s
    }
}

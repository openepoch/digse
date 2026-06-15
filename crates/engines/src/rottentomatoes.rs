//! Rotten Tomatoes search engine implementation
//!
//! The search
//! results page exposes `<search-page-media-row>` custom elements carrying
//! metadata as attributes (release year, score, cast).

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Rotten Tomatoes search engine (general / movies, HTML scrape)
pub struct RottenTomatoesEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl RottenTomatoesEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "rottentomatoes".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Rotten Tomatoes - movie/TV review aggregator.".to_string(),
            website: Some("https://www.rottentomatoes.com/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Rotten Tomatoes HTTP client");
        RottenTomatoesEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://www.rottentomatoes.com";
        let encoded = urlencoding::encode(&query.query);
        let url = format!("{}/search?search={}", base_url, encoded);
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let html = resp
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        Ok(self.parse(&html))
    }

    fn parse(&self, html: &str) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        let row_sel = match Selector::parse("search-page-media-row") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let img_sel = Selector::parse("a img").unwrap();
        let a_sel = Selector::parse("a").unwrap();

        for row in document.select(&row_sel) {
            let release_year = row.value().attr("releaseyear").unwrap_or("").trim();
            let score = row.value().attr("tomatometerscore").unwrap_or("").trim();
            let cast = row.value().attr("cast").unwrap_or("").trim();

            let href = row
                .select(&a_sel)
                .next()
                .and_then(|a| a.value().attr("href"))
                .unwrap_or("")
                .to_string();
            let (title, thumbnail) = match row.select(&img_sel).next() {
                Some(img) => (
                    img.value().attr("alt").unwrap_or("").to_string(),
                    img.value().attr("src").unwrap_or("").to_string(),
                ),
                None => (String::new(), String::new()),
            };
            if href.is_empty() && title.is_empty() {
                continue;
            }
            let mut content_parts = Vec::new();
            if !release_year.is_empty() {
                content_parts.push(format!("From {}", release_year));
            }
            if !score.is_empty() {
                content_parts.push(format!("Score: {}", score));
            }
            if !cast.is_empty() {
                content_parts.push(format!("Starring {}", cast));
            }
            let mut r = SearchResult::new(title, href)
                .with_snippet(content_parts.join(", "))
                .with_engine(self.name())
                .with_result_type(ResultType::Web);
            if !thumbnail.is_empty() {
                r = r.with_extra("thumbnail", serde_json::json!(thumbnail));
            }
            results.push(r);
        }
        results
    }
}

#[async_trait]
impl Engine for RottenTomatoesEngine {
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
        let mut results = self.fetch_results(query).await?;
        for (i, r) in results.iter_mut().enumerate() {
            r.rank = query.offset + i + 1;
            r.score = 1.0 - (i as f64 * 0.05);
        }
        Ok(results)
    }
    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Web | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://www.rottentomatoes.com".into());
        s
    }
}

//! Google Play (Apps & Movies) search engine implementation
//!
//! Scrapes the Google Play Store
//! search results. `play_categ` selects between "apps" and "movies".

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};
use scraper::{Html, Selector};

/// Google Play search engine (apps by default)
pub struct GooglePlayEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    play_categ: String,
    result_type: ResultType,
}

impl GooglePlayEngine {
    pub fn new() -> Self {
        Self::with_categ("apps")
    }

    pub fn with_categ(categ: &str) -> Self {
        let (category, description, result_type) = match categ {
            "movies" => (
                EngineCategory::Videos,
                "Google Play Movies - movies & TV search.".to_string(),
                ResultType::Videos,
            ),
            _ => (
                EngineCategory::IT,
                "Google Play Apps - Android app search.".to_string(),
                ResultType::IT,
            ),
        };
        let metadata = EngineMetadata {
            name: "google_play".to_string(),
            category,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description,
            website: Some("https://play.google.com".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Google Play HTTP client");
        GooglePlayEngine {
            metadata,
            client,
            play_categ: categ.to_string(),
            result_type,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if self.play_categ != "movies" && self.play_categ != "apps" {
            return Ok(vec![]);
        }
        let url = format!(
            "https://play.google.com/store/search?q={}&c={}",
            urlencoding::encode(&query.query),
            self.play_categ
        );
        let response = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (digse/0.1.0)")
            .header("Accept", "text/html,application/xhtml+xml")
            .header("Cookie", "CONSENT=YES+")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        if !response.status().is_success() {
            return Ok(vec![]);
        }
        let text = response
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        if self.play_categ == "movies" {
            Ok(self.parse_movies(&text, query))
        } else {
            Ok(self.parse_apps(&text, query))
        }
    }

    fn parse_movies(&self, html_text: &str, query: &SearchQuery) -> Vec<SearchResult> {
        let doc = Html::parse_document(html_text);
        let mut results = Vec::new();
        let a_sel = Selector::parse("a").unwrap();
        let div_title_sel = Selector::parse("div[title]").unwrap();
        let img_sel = Selector::parse("img").unwrap();

        // iterate all <a> with hrefs that look like movie entries
        for (i, a) in doc.select(&a_sel).enumerate() {
            if i >= query.count {
                break;
            }
            let href = a.value().attr("href").unwrap_or("");
            if href.is_empty() || !href.contains("/store/movies/") {
                continue;
            }
            let url = if href.starts_with('/') {
                format!("https://play.google.com{}", href)
            } else {
                format!("https://play.google.com/{}", href)
            };
            let title = a
                .select(&div_title_sel)
                .next()
                .map(|d| d.value().attr("title").unwrap_or("").to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| a.text().collect::<String>().trim().to_string());
            if title.is_empty() {
                continue;
            }
            let thumbnail = a
                .select(&img_sel)
                .next()
                .and_then(|img| img.value().attr("src"))
                .unwrap_or("")
                .to_string();

            let result = SearchResult::new(title, url)
                .with_snippet("Google Play Movies".to_string())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Videos)
                .with_extra("thumbnail", serde_json::json!(thumbnail));
            results.push(result);
        }
        results
    }

    fn parse_apps(&self, html_text: &str, query: &SearchQuery) -> Vec<SearchResult> {
        let doc = Html::parse_document(html_text);
        let mut results = Vec::new();
        // The "no results" container
        if let Ok(sel) = Selector::parse("div.v6DsQb") {
            if doc.select(&sel).next().is_some() {
                return results;
            }
        }
        let a_sel = Selector::parse("a").unwrap();
        let span_title_sel = Selector::parse("span.DdYX5").unwrap();
        let span_desc_sel = Selector::parse("span.wMUdtb").unwrap();
        let img_sel = Selector::parse("img").unwrap();

        for (i, a) in doc.select(&a_sel).enumerate() {
            if results.len() >= query.count {
                break;
            }
            let href = a.value().attr("href").unwrap_or("");
            if href.is_empty() || !href.contains("/store/apps/details") {
                continue;
            }
            let url = if href.starts_with('/') {
                format!("https://play.google.com{}", href)
            } else {
                format!("https://play.google.com/{}", href)
            };
            let title = a
                .select(&span_title_sel)
                .next()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let content = a
                .select(&span_desc_sel)
                .next()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let img_src = a
                .select(&img_sel)
                .next()
                .and_then(|img| img.value().attr("src"))
                .unwrap_or("")
                .to_string();

            let result = SearchResult::new(title, url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + results.len() + 1)
                .with_score(1.0 - (results.len() as f64 * 0.05))
                .with_result_type(ResultType::IT)
                .with_extra("img_src", serde_json::json!(img_src));
            results.push(result);
            let _ = i; // suppress unused
        }
        results
    }
}

#[async_trait]
impl Engine for GooglePlayEngine {
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
        matches!(t, ResultType::IT | ResultType::Videos | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://play.google.com".into());
        s.insert("play_categ".into(), self.play_categ.clone());
        s
    }
}

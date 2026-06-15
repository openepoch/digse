//! SourceHut search engine implementation
//!
//! Scrapes the SourceHut
//! public project listing at `https://sr.ht/projects`.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// SourceHut (code/repo) search engine
pub struct SourceHutEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://sr.ht/projects";

impl SourceHutEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "sourcehut".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "SourceHut - collaborative software platform project search.".to_string(),
            website: Some("https://sourcehut.org".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create SourceHut HTTP client");

        SourceHutEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let page = ((query.offset / 10) + 1).to_string();
        let resp = self
            .client
            .get(BASE_URL)
            .header("User-Agent", "digse/0.0.1")
            .query(&[
                ("search", query.query.as_str()),
                ("page", page.as_str()),
                ("sort", "recently-updated"),
            ])
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
        Ok(self.parse_html(html, query))
    }

    fn parse_html(&self, html: String, query: &SearchQuery) -> Vec<SearchResult> {
        use scraper::{Html, Selector};

        let document = Html::parse_document(&html);
        let mut results = Vec::new();

        // First event list only.
        let list_sel = match Selector::parse("div.event-list") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let event_sel = Selector::parse("div.event").unwrap();
        let h4_sel = Selector::parse("h4").unwrap();
        let h4_a_sel = Selector::parse("h4 a").unwrap();
        let p_sel = Selector::parse("p").unwrap();
        let tags_sel = Selector::parse("div[class*='tags'] a").unwrap();

        // Take only the first event list.
        let first_list = match document.select(&list_sel).next() {
            Some(l) => l,
            None => return results,
        };

        for (i, item) in first_list.select(&event_sel).enumerate() {
            // title: full h4 text
            let title = item
                .select(&h4_sel)
                .next()
                .map(|h| h.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            // second <a> inside h4 is the project link (first is maintainer).
            let h4_anchors: Vec<_> = item.select(&h4_a_sel).collect();
            let (package_name, maintainer) = if h4_anchors.len() >= 2 {
                let pkg = h4_anchors[1]
                    .text()
                    .collect::<String>()
                    .trim()
                    .to_string();
                let maint = h4_anchors[0]
                    .text()
                    .collect::<String>()
                    .trim()
                    .trim_start_matches('~')
                    .to_string();
                (pkg, maint)
            } else if let Some(a) = h4_anchors.first() {
                (
                    a.text().collect::<String>().trim().to_string(),
                    String::new(),
                )
            } else {
                (String::new(), String::new())
            };

            let href = h4_anchors
                .get(1)
                .and_then(|a| a.value().attr("href").map(|s| s.to_string()))
                .unwrap_or_default();
            let url = if href.starts_with("http") {
                href
            } else {
                format!("https://sr.ht{}", if href.starts_with('/') { href.clone() } else { format!("/{}", href) })
            };

            let content = item
                .select(&p_sel)
                .next()
                .map(|p| p.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let tags: Vec<String> = item
                .select(&tags_sel)
                .map(|t| {
                    t.text()
                        .collect::<String>()
                        .trim()
                        .trim_start_matches('#')
                        .to_string()
                })
                .filter(|t| !t.is_empty())
                .collect();

            if title.is_empty() && package_name.is_empty() {
                continue;
            }

            let mut snippet = content.clone();
            if !maintainer.is_empty() {
                snippet = if snippet.is_empty() {
                    format!("Maintainer: {}", maintainer)
                } else {
                    format!("{} | Maintainer: {}", snippet, maintainer)
                };
            }
            if !tags.is_empty() {
                snippet = if snippet.is_empty() {
                    format!("Tags: {}", tags.join(", "))
                } else {
                    format!("{} | Tags: {}", snippet, tags.join(", "))
                };
            }

            let mut result = SearchResult::new(
                if title.is_empty() { package_name.clone() } else { title },
                url,
            )
            .with_snippet(snippet)
            .with_engine(self.name())
            .with_rank(query.offset + i + 1)
            .with_score(1.0 - (i as f64 * 0.05))
            .with_result_type(ResultType::IT);

            if !package_name.is_empty() {
                result = result.with_extra("package_name", serde_json::json!(package_name));
            }
            if !maintainer.is_empty() {
                result = result.with_extra("maintainer", serde_json::json!(maintainer));
            }
            if !tags.is_empty() {
                result = result.with_extra("tags", serde_json::json!(tags));
            }

            results.push(result);
        }

        results
    }
}

#[async_trait]
impl Engine for SourceHutEngine {
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
        matches!(t, ResultType::IT | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), BASE_URL.into());
        s.insert("sort".into(), "recently-updated".into());
        s.insert("results".into(), "HTML".into());
        s
    }
}

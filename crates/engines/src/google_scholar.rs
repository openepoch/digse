//! Google Scholar search engine implementation
//!
//! Academic HTML scrape of
//! scholar.google.com. Score step is 0.03 (academic).

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};
use scraper::{Html, Selector};

/// Google Scholar search engine
pub struct GoogleScholarEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl GoogleScholarEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "google_scholar".to_string(),
            category: EngineCategory::Science,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Google Scholar - scholarly literature search.".to_string(),
            website: Some("https://scholar.google.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Google Scholar HTTP client");

        GoogleScholarEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let start = query.offset;
        let url = format!(
            "https://scholar.google.com/scholar?q={}&start={}&as_sdt=2007&as_vis=0",
            urlencoding::encode(&query.query),
            start
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (digse/0.0.1)")
            .header("Accept", "text/html,application/xhtml+xml")
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

        Ok(self.parse(&text, query))
    }

    fn parse(&self, html_text: &str, query: &SearchQuery) -> Vec<SearchResult> {
        let doc = Html::parse_document(html_text);
        let mut results = Vec::new();

        let result_sel = match Selector::parse("div[data-rp]") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let h3_a_sel = Selector::parse("h3 a").unwrap();
        let content_sel = Selector::parse("div.gs_rs").unwrap();
        let gs_a_sel = Selector::parse("div.gs_a").unwrap();
        let cites_sel = Selector::parse("div.gs_fl a[href*='/scholar?cites=']").unwrap();
        let pdf_sel = Selector::parse("div.gs_or_ggsm a").unwrap();
        let pub_type_sel = Selector::parse("span.gs_ctg2").unwrap();

        for (i, el) in doc.select(&result_sel).enumerate() {
            let a = match el.select(&h3_a_sel).next() {
                Some(a) => a,
                None => continue, // citation-only block
            };
            let title = a.text().collect::<String>().trim().to_string();
            if title.is_empty() {
                continue;
            }
            let url = a.value().attr("href").unwrap_or("").to_string();
            if url.is_empty() {
                continue;
            }

            let pub_type = el
                .select(&pub_type_sel)
                .next()
                .map(|s| {
                    let raw = s.text().collect::<String>();
                    // strip surrounding [ ] and lowercase
                    let t = raw.trim();
                    if t.starts_with('[') && t.ends_with(']') {
                        t[1..t.len() - 1].to_lowercase()
                    } else {
                        t.to_lowercase()
                    }
                })
                .unwrap_or_default();

            let content = el
                .select(&content_sel)
                .next()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let gs_a = el
                .select(&gs_a_sel)
                .next()
                .map(|s| s.text().collect::<String>())
                .unwrap_or_default();
            let (authors, journal, publisher, year) = parse_gs_a(&gs_a);

            let comments = el
                .select(&cites_sel)
                .next()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let pdf_url = el
                .select(&pdf_sel)
                .next()
                .and_then(|a| a.value().attr("href"))
                .unwrap_or("")
                .to_string();

            let mut snippet_parts = Vec::new();
            if !authors.is_empty() {
                snippet_parts.push(format!("Authors: {}", authors.join(", ")));
            }
            if !journal.is_empty() {
                snippet_parts.push(format!("Journal: {}", journal));
            }
            if !publisher.is_empty() && !url.contains(&publisher) {
                snippet_parts.push(format!("Publisher: {}", publisher));
            }
            if let Some(y) = year {
                snippet_parts.push(format!("Year: {}", y));
            }
            if !content.is_empty() {
                snippet_parts.push(content.clone());
            }
            if !comments.is_empty() {
                snippet_parts.push(comments.clone());
            }
            let snippet = snippet_parts.join(" | ");

            let mut result = SearchResult::new(title, url)
                .with_snippet(snippet)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.03))
                .with_result_type(ResultType::Academic);
            if !authors.is_empty() {
                result = result.with_extra("authors", serde_json::json!(authors));
            }
            if let Some(y) = year {
                result = result.with_extra("year", serde_json::json!(y));
            }
            if !pdf_url.is_empty() {
                result = result.with_extra("pdf_url", serde_json::json!(pdf_url));
            }
            if !comments.is_empty() {
                result = result.with_extra("citations", serde_json::json!(comments));
            }
            if !pub_type.is_empty() {
                result = result.with_extra("type", serde_json::json!(pub_type));
            }
            results.push(result);
        }

        results
    }
}

/// Parse the green "gs_a" text into authors, journal, publisher, year.
/// Formats:
///   "{authors} - {journal}, {year} - {publisher}"
///   "{authors} - {year} - {publisher}"
///   "{authors} - {publisher}"
fn parse_gs_a(text: &str) -> (Vec<String>, String, String, Option<i32>) {
    if text.trim().is_empty() {
        return (Vec::new(), String::new(), String::new(), None);
    }
    let parts: Vec<&str> = text.split(" - ").collect();
    let authors: Vec<String> = parts[0].split(", ").map(|s| s.trim().to_string()).collect();
    let publisher = parts.last().unwrap_or(&"").trim().to_string();
    if parts.len() != 3 {
        return (authors, String::new(), publisher, None);
    }
    let journal_year: Vec<&str> = parts[1].split(", ").collect();
    let journal = if journal_year.len() > 1 {
        let j = journal_year[..journal_year.len() - 1].join(", ");
        if j == "…" {
            String::new()
        } else {
            j
        }
    } else {
        String::new()
    };
    let year_str = journal_year.last().unwrap_or(&"").trim();
    let year = year_str.parse::<i32>().ok();
    (authors, journal, publisher, year)
}

#[async_trait]
impl Engine for GoogleScholarEngine {
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
        matches!(t, ResultType::Academic | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://scholar.google.com".into());
        s
    }
}

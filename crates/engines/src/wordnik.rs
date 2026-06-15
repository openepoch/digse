//! Wordnik dictionary search engine implementation (HTML).

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Wordnik online dictionary engine.
pub struct WordnikEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl WordnikEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "wordnik".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Wordnik - online dictionary word definitions.".to_string(),
            website: Some("https://www.wordnik.com".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Wordnik HTTP client");
        WordnikEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let word = query.query.trim();
        if word.is_empty() {
            return Ok(vec![]);
        }
        let url = format!("https://www.wordnik.com/words/{}", urlencoding::encode(word));

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
        let html = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        Ok(self.parse_html(&html, &url, word, query))
    }

    fn parse_html(&self, html: &str, url: &str, word: &str, query: &SearchQuery) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        // upstream: //*[@id="define"]//h3[@class="source"]
        let source_sel = match Selector::parse("#define h3.source, #define .source") {
            Ok(s) => s,
            Err(_) => return results,
        };

        // Collect source headings; then look at following sibling <ul> lists.
        let sources: Vec<scraper::ElementRef> = document.select(&source_sel).collect();
        if sources.is_empty() {
            return results;
        }

        let li_sel = match Selector::parse("li") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let abbr_sel = match Selector::parse("abbr") {
            Ok(s) => s,
            Err(_) => return results,
        };

        for (i, src) in sources.iter().enumerate() {
            if i >= query.count {
                break;
            }
            let source_name = src.text().collect::<String>().trim().to_string();

            // find the first following sibling <ul>
            let mut defs: Vec<String> = Vec::new();
            if let Some(parent) = src.parent() {
                // walk siblings after this h3.source within the same parent
                let siblings = parent.children();
                let mut found = false;
                for sib in siblings {
                    if sib.id() == src.id() {
                        found = true;
                        continue;
                    }
                    if !found {
                        continue;
                    }
                    if let Some(el_ref) = scraper::ElementRef::wrap(sib) {
                        if el_ref.value().name() == "ul" {
                            for li in el_ref.select(&li_sel) {
                                let abbr = li
                                    .select(&abbr_sel)
                                    .next()
                                    .map(|a| a.text().collect::<String>().trim().to_string())
                                    .unwrap_or_default();
                                let mut def = li.text().collect::<String>().trim().to_string();
                                if !abbr.is_empty() && def.starts_with(&abbr) {
                                    def = def[abbr.len()..].trim().to_string();
                                }
                                if !def.is_empty() {
                                    defs.push(def);
                                }
                            }
                            break;
                        }
                    }
                }
            }

            if defs.is_empty() {
                continue;
            }
            let title = format!("{} ({})", word, source_name);
            let snippet = defs.join("; ");
            let r = SearchResult::new(title, url.to_string())
                .with_snippet(snippet.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web)
                .with_extra("word", serde_json::json!(word))
                .with_extra("source", serde_json::json!(source_name))
                .with_extra("definitions", serde_json::json!(defs));
            results.push(r);
        }
        results
    }
}

#[async_trait]
impl Engine for WordnikEngine {
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
        s.insert("base_url".to_string(), "https://www.wordnik.com".to_string());
        s.insert("results".to_string(), "HTML".to_string());
        s
    }
}

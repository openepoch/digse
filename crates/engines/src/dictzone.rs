//! Dictzone dictionary search engine implementation
//!
//! an online dictionary that scrapes
//! dictzone.com translation pages. The reference parses an HTML table with id
//! `r` and returns translation rows.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Dictzone dictionary engine
pub struct DictzoneEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl DictzoneEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "dictzone".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "Dictzone online dictionary.".to_string(),
            website: Some("https://dictzone.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create Dictzone HTTP client");

        DictzoneEngine { metadata, client }
    }

    /// Parse translation rows from the dictzone HTML table.
    fn parse_html(&self, html: &str) -> Vec<(String, String, Vec<String>)> {
        let doc = Html::parse_document(html);
        let mut out = Vec::new();

        let tr_sel = match Selector::parse("table#r tr") {
            Ok(s) => s,
            Err(_) => return out,
        };
        let td_sel = Selector::parse("td").unwrap();
        let p_sel = Selector::parse("p").unwrap();
        let smpl_sel = Selector::parse("i.smpl").unwrap();

        for tr in doc.select(&tr_sel) {
            let tds: Vec<_> = tr.select(&td_sel).collect();
            if tds.len() != 2 {
                continue;
            }
            let col_from = tds[0].text().collect::<String>().trim().to_string();
            if col_from.is_empty() {
                continue;
            }
            let mut text = col_from.clone();
            let mut synonyms: Vec<String> = Vec::new();
            let p_items: Vec<_> = tds[1].select(&p_sel).collect();
            for (i, p_item) in p_items.iter().enumerate() {
                let mut p_text = p_item.text().collect::<String>().trim().to_string();
                let smpl = p_item
                    .select(&smpl_sel)
                    .next()
                    .map(|e| e.text().collect::<String>().trim().to_string())
                    .unwrap_or_default();
                if !smpl.is_empty() {
                    p_text = format!("{} // {}", p_text, smpl);
                }
                if p_text.is_empty() {
                    continue;
                }
                if i == 0 {
                    text = format!("{} : {}", text, p_text);
                } else {
                    synonyms.push(p_text);
                }
            }
            out.push((text, tds[1].text().collect::<String>().trim().to_string(), synonyms));
        }
        out
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        // Parse "<word> <from> <to>" e.g. "hello english german" or default en->de
        let tokens: Vec<&str> = query.query.split_whitespace().collect();
        let (from_lang, to_lang, word) = if tokens.len() >= 3 {
            (
                tokens[tokens.len() - 2].to_string(),
                tokens[tokens.len() - 1].to_string(),
                tokens[..tokens.len() - 2].join(" "),
            )
        } else if tokens.len() == 1 {
            ("english".to_string(), "german".to_string(), tokens[0].to_string())
        } else {
            ("english".to_string(), "german".to_string(), query.query.clone())
        };
        if word.is_empty() {
            return Ok(vec![]);
        }

        let encoded = urlencoding::encode(&word);
        let url = format!(
            "https://dictzone.com/{}-{}-dictionary/{}",
            from_lang, to_lang, encoded
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "text/html")
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

        let parsed = self.parse_html(&text);
        let mut results = Vec::new();
        for (i, (text_row, _col_to, synonyms)) in parsed.iter().enumerate() {
            let result = SearchResult::new(text_row.clone(), url.clone())
                .with_snippet(text_row.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web)
                .with_extra("translation", serde_json::json!(text_row))
                .with_extra("synonyms", serde_json::json!(synonyms));
            results.push(result);
            if results.len() >= query.count {
                break;
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for DictzoneEngine {
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

    fn supports_result_type(&self, result_type: &ResultType) -> bool {
        matches!(result_type, ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "https://dictzone.com".to_string());
        settings.insert("engine_type".to_string(), "online_dictionary".to_string());
        settings
    }
}

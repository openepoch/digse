//! Arxiv search engine implementation

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Arxiv search engine
pub struct ArxivEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl ArxivEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "arxiv".to_string(),
            category: EngineCategory::Science,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Arxiv academic paper search".to_string(),
            website: Some("https://arxiv.org".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Arxiv HTTP client");

        ArxivEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = format!(
            "http://export.arxiv.org/api/query?search_query=all:{}&start={}&max_results={}",
            urlencoding::encode(&query.query),
            query.offset,
            query.count
        );


        let response = self.client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/xml")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;


        if !response.status().is_success() {
            return Err(Error::EngineError(
                "arxiv".to_string(),
                format!("HTTP error: {}", response.status()),
            ));
        }

        let text = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;

        // Parse XML - simplified approach, just extract key information
        let results = self.parse_arxiv_xml(&text)?;

        Ok(results)
    }

    fn parse_arxiv_xml(&self, xml: &str) -> Result<Vec<SearchResult>> {
        let mut results = Vec::new();

        // Simple XML parsing - look for entry tags
        let mut entry_start = 0;
        while let Some(entry_pos) = xml[entry_start..].find("<entry>") {
            let entry_start_absolute = entry_start + entry_pos + 7; // Skip past <entry>
            let entry_end = xml[entry_start_absolute..].find("</entry>")
                .map(|e| entry_start_absolute + e)
                .unwrap_or(xml.len());

            let entry_content = &xml[entry_start_absolute..entry_end];

            // Extract title
            let title = self.extract_xml_tag(entry_content, "title")
                .unwrap_or_default()
                .chars()
                .take(200)
                .collect::<String>()
                .trim()
                .to_string();

            // Extract ID
            let arxiv_id = self.extract_xml_tag(entry_content, "id")
                .unwrap_or_default()
                .trim()
                .to_string();

            // Extract URL (from arxiv_id)
            let url = if arxiv_id.starts_with("http") {
                arxiv_id.clone()
            } else if !arxiv_id.is_empty() {
                format!("https://arxiv.org/abs/{}", arxiv_id)
            } else {
                continue;
            };

            // Extract summary
            let summary = self.extract_xml_tag(entry_content, "summary")
                .unwrap_or_default()
                .chars()
                .take(500)
                .collect::<String>()
                .trim()
                .to_string();

            // Extract authors
            let authors = self.extract_authors(entry_content);

            // Extract primary category
            let primary_category = self.extract_xml_tag(entry_content, "primary_category")
                .unwrap_or_default()
                .trim()
                .to_string();

            // Create content string
            let content = if !authors.is_empty() {
                format!("{} - {} | {}", summary, authors.join(", "), primary_category)
            } else {
                format!("{} | {}", summary, primary_category)
            };

            if !title.is_empty() && !url.is_empty() {
                let result = SearchResult::new(&title, &url)
                    .with_snippet(&content)
                    .with_engine("arxiv")
                    .with_score(1.0)
                    .with_extra("arxiv_id", serde_json::json!(arxiv_id))
                    .with_extra("primary_category", serde_json::json!(primary_category))
                    .with_extra("authors", serde_json::json!(authors.join(", ")));

                results.push(result);
            }

            entry_start = entry_end + 8; // Move past </entry>
        }

        Ok(results)
    }

    fn extract_xml_tag(&self, content: &str, tag: &str) -> Option<String> {
        let start_tag = format!("<{}>", tag);
        let end_tag = format!("</{}>", tag);

        if let Some(start) = content.find(&start_tag) {
            if let Some(end) = content.find(&end_tag) {
                let inner = &content[start + start_tag.len()..end];
                // Remove HTML entities and normalize whitespace
                return Some(inner.trim().to_string());
            }
        }
        None
    }

    fn extract_authors(&self, entry_content: &str) -> Vec<String> {
        let mut authors = Vec::new();
        let mut search_start = 0;

        while let Some(name_start) = entry_content[search_start..].find("<name>") {
            let name_start_absolute = search_start + name_start + 6;
            if let Some(name_end) = entry_content[name_start_absolute..].find("</name>") {
                let name = entry_content[name_start_absolute..name_start_absolute + name_end].trim();
                if !name.is_empty() {
                    authors.push(name.to_string());
                }
                search_start = name_start_absolute + name_end + 7;
            } else {
                break;
            }
        }

        authors
    }
}

#[async_trait]
impl Engine for ArxivEngine {
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
        *result_type == ResultType::Academic || *result_type == ResultType::All
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "https://arxiv.org".to_string());
        settings.insert("api_endpoint".to_string(), "/api/query".to_string());
        settings.insert("format".to_string(), "xml".to_string());
        settings
    }
}

//! Mojeek search engine implementation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Mojeek search engine
pub struct MojeekEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    search_type: MojeekSearchType,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MojeekSearchType {
    General,
    Images,
    News,
}

#[derive(Debug, Serialize, Deserialize)]
struct MojeekWebResult {
    url: String,
    title: String,
    content: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct MojeekImageResult {
    url: String,
    title: String,
    img_src: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct MojeekNewsResult {
    url: String,
    title: String,
    content: String,
}

impl MojeekEngine {
    pub fn new(search_type: MojeekSearchType) -> Self {
        let metadata = Self::create_metadata(&search_type);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create Mojeek HTTP client");

        MojeekEngine { metadata, client, search_type }
    }

    fn create_metadata(search_type: &MojeekSearchType) -> EngineMetadata {
        match search_type {
            MojeekSearchType::General => EngineMetadata {
                name: "mojeek_general".to_string(),
                category: EngineCategory::General,
                enabled: true,
                requires_auth: false,
                timeout_seconds: 10,
                description: "Mojeek general web search".to_string(),
                website: Some("https://mojeek.com".to_string()),
            },
            MojeekSearchType::Images => EngineMetadata {
                name: "mojeek_images".to_string(),
                category: EngineCategory::Images,
                enabled: true,
                requires_auth: false,
                timeout_seconds: 10,
                description: "Mojeek image search".to_string(),
                website: Some("https://mojeek.com".to_string()),
            },
            MojeekSearchType::News => EngineMetadata {
                name: "mojeek_news".to_string(),
                category: EngineCategory::News,
                enabled: true,
                requires_auth: false,
                timeout_seconds: 10,
                description: "Mojeek news search".to_string(),
                website: Some("https://mojeek.com".to_string()),
            },
        }
    }

    pub fn new_general() -> Self {
        Self::new(MojeekSearchType::General)
    }

    pub fn new_images() -> Self {
        Self::new(MojeekSearchType::Images)
    }

    pub fn new_news() -> Self {
        Self::new(MojeekSearchType::News)
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://www.mojeek.com";
        let offset = query.offset.to_string();

        let mut params = vec![
            ("q", query.query.as_str()),
            ("safe", "1"),
        ];

        // Add search type parameter
        match self.search_type {
            MojeekSearchType::General => {},
            MojeekSearchType::Images => params.push(("fmt", "images")),
            MojeekSearchType::News => params.push(("fmt", "news")),
        }

        // Add pagination for general search
        if self.search_type == MojeekSearchType::General && query.offset > 0 {
            params.push(("s", offset.as_str()));
        }

        let url = format!("{}?{}", base_url,
            params.into_iter()
                .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
                .collect::<Vec<_>>()
                .join("&"));


        let response = self.client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .header("Accept-Language", "en-US,en;q=0.5")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;


        if !response.status().is_success() {
            return Err(Error::EngineError(
                "mojeek".to_string(),
                format!("HTTP error: {}", response.status()),
            ));
        }

        let html = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;

        let results = self.parse_html(&html)?;

        Ok(results)
    }

    fn parse_html(&self, html: &str) -> Result<Vec<SearchResult>> {
        use scraper::{Html, Selector};

        let document = Html::parse_document(html);
        let mut results = Vec::new();

        match self.search_type {
            MojeekSearchType::General => {
                // Parse general web results - simplified approach
                let link_selector = Selector::parse("ul.results-standard a.ob")
                    .map_err(|_| Error::ParseError("Invalid link selector".to_string()))?;

                for element in document.select(&link_selector) {
                    let url = element.value().attr("href").unwrap_or("");

                    if !url.is_empty() && url.starts_with("http") {
                        let title = element.text().collect::<String>();
                        let content = String::new(); // Would need to find associated content element

                        results.push(
                            SearchResult::new(title.trim(), url)
                                .with_snippet(content.trim())
                                .with_engine(self.name())
                                .with_score(1.0)
                        );
                    }
                }
            },
            MojeekSearchType::Images => {
                // Parse image results
                let image_selector = Selector::parse("#results div.image")
                    .map_err(|_| Error::ParseError("Invalid image selector".to_string()))?;

                for element in document.select(&image_selector) {
                    let link_elem = element.select(&Selector::parse("a").unwrap()).next();
                    let img_elem = element.select(&Selector::parse("img").unwrap()).next();

                    if let (Some(link), Some(img)) = (link_elem, img_elem) {
                        let url = link.value().attr("href").unwrap_or("");
                        let title = link.value().attr("data-title").unwrap_or("");
                        let img_src = img.value().attr("src").unwrap_or("");

                        if !url.is_empty() {
                            let full_img_src = if img_src.starts_with("http") {
                                img_src.to_string()
                            } else {
                                format!("https://www.mojeek.com{}", img_src)
                            };

                            let result = SearchResult::new(title.trim(), url)
                                .with_extra("image_url", serde_json::json!(full_img_src))
                                .with_engine(self.name())
                                .with_score(1.0);
                            results.push(result);
                        }
                    }
                }
            },
            MojeekSearchType::News => {
                // Parse news results
                let news_selector = Selector::parse("section.news-search-result article")
                    .map_err(|_| Error::ParseError("Invalid news selector".to_string()))?;

                for element in document.select(&news_selector) {
                    let link_elem = element.select(&Selector::parse("h2 a").unwrap()).next();
                    let content_elem = element.select(&Selector::parse("p.s").unwrap()).next();

                    if let Some(link) = link_elem {
                        let url = link.value().attr("href").unwrap_or("");
                        let title = link.text().collect::<String>();
                        let content = content_elem.map(|e| e.text().collect::<String>()).unwrap_or_default();

                        if !url.is_empty() && url.starts_with("http") {
                            results.push(
                                SearchResult::new(title.trim(), url)
                                    .with_snippet(content.trim())
                                    .with_engine(self.name())
                                    .with_score(1.0)
                            );
                        }
                    }
                }
            },
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for MojeekEngine {
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
        match self.search_type {
            MojeekSearchType::General => *result_type == ResultType::Web || *result_type == ResultType::All,
            MojeekSearchType::Images => *result_type == ResultType::Images || *result_type == ResultType::All,
            MojeekSearchType::News => *result_type == ResultType::News || *result_type == ResultType::All,
        }
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "https://mojeek.com".to_string());
        settings.insert("search_type".to_string(), format!("{:?}", self.search_type));
        settings
    }
}

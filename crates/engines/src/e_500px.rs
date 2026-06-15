//! 500px search engine implementation (GraphQL JSON, images)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// 500px photography search engine
pub struct Px500Engine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Px500Response {
    #[serde(default)]
    data: Px500Data,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Px500Data {
    #[serde(default)]
    #[serde(rename = "photoSearch")]
    photo_search: Px500PhotoSearch,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Px500PhotoSearch {
    #[serde(default)]
    edges: Vec<Px500Edge>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Px500Edge {
    #[serde(default)]
    node: Px500Node,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Px500Node {
    #[serde(default)]
    id: String,
    #[serde(default, rename = "canonicalPath")]
    canonical_path: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    width: i64,
    #[serde(default)]
    height: i64,
    #[serde(default)]
    photographer: Px500Uploader,
    #[serde(default)]
    images: Vec<Px500Image>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Px500Uploader {
    #[serde(default, rename = "displayName")]
    display_name: String,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
struct Px500Image {
    #[serde(default)]
    size: i64,
    #[serde(default)]
    url: String,
}

impl Px500Engine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "500px".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "500px - photography community image search.".to_string(),
            website: Some("https://500px.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create 500px HTTP client");

        Px500Engine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://500px.com";
        let api_url = "https://api.500px.com/graphql";
        let first = query.count.min(30);

        let gql = r#"query PhotoSearchPaginationContainerQuery($first: Int, $cursor: String, $search: String!, $sort: PhotoSort, $filters: [PhotoSearchFilter!], $nlp: Boolean) {
  photoSearch(sort: $sort, first: $first, after: $cursor, search: $search, filters: $filters, nlp: $nlp) {
    edges { node { id canonicalPath name description width height photographer: uploader { displayName } images(sizes: [35, 33]) { size url } } cursor }
  }
}"#;

        let body = serde_json::json!({
            "operationName": "PhotoSearchPaginationContainerQuery",
            "variables": {
                "first": first,
                "cursor": null,
                "search": query.query,
                "sort": "RELEVANCE",
                "filters": [],
                "nlp": false,
            },
            "query": gql,
        });

        let resp = self.client
            .post(api_url)
            .header("User-Agent", "digse/0.1.0")
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: Px500Response = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, edge) in parsed.data.photo_search.edges.iter().enumerate() {
            let node = &edge.node;
            if node.images.is_empty() {
                continue;
            }
            let mut imgs = node.images.clone();
            imgs.sort_by_key(|im| im.size);
            let thumbnail = imgs.first().map(|i| i.url.clone()).unwrap_or_default();
            let img_src = imgs.last().map(|i| i.url.clone()).unwrap_or_default();
            let url = format!("{}{}", base_url, node.canonical_path);

            let r = SearchResult::new(node.name.clone(), url)
                .with_snippet(node.description.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Images)
                .with_extra("img_src", serde_json::json!(img_src))
                .with_extra("thumbnail", serde_json::json!(thumbnail))
                .with_extra("source", serde_json::json!("500px"))
                .with_extra("author", serde_json::json!(node.photographer.display_name))
                .with_extra("width", serde_json::json!(node.width))
                .with_extra("height", serde_json::json!(node.height));
            results.push(r);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for Px500Engine {
    fn name(&self) -> &str { &self.metadata.name }
    fn category(&self) -> EngineCategory { self.metadata.category }
    fn is_enabled(&self) -> bool { self.metadata.enabled }
    fn metadata(&self) -> EngineMetadata { self.metadata.clone() }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        self.fetch_results(query).await
    }

    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Images | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://500px.com".to_string());
        s.insert("api_url".to_string(), "https://api.500px.com/graphql".to_string());
        s
    }
}

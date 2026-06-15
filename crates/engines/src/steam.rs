//! Steam store search engine implementation
//!
//! Queries the Steam
//! store search JSON API.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Steam store search engine
pub struct SteamEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://store.steampowered.com";

#[derive(Debug, Serialize, Deserialize, Default)]
struct SteamResponse {
    #[serde(default)]
    items: Vec<SteamItem>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SteamItem {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    #[serde(rename = "tiny_image")]
    tiny_image: String,
    #[serde(default)]
    price: SteamPrice,
    #[serde(default)]
    platforms: SteamPlatforms,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SteamPrice {
    #[serde(default)]
    currency: String,
    #[serde(default)]
    final_price: i64,
}

// The platforms object looks like {"windows": true, "mac": false, "linux": true}.
#[derive(Debug, Serialize, Deserialize, Default)]
struct SteamPlatforms {
    #[serde(default)]
    windows: bool,
    #[serde(default)]
    mac: bool,
    #[serde(default)]
    linux: bool,
}

impl SteamEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "steam".to_string(),
            category: EngineCategory::Shopping,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Steam - digital game store search.".to_string(),
            website: Some("https://store.steampowered.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Steam HTTP client");

        SteamEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let endpoint = format!("{}/api/storesearch/", BASE_URL);
        let resp = self
            .client
            .get(&endpoint)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("term", query.query.as_str()),
                ("cc", "us"),
                ("l", "en"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        let parsed: SteamResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.items.iter().enumerate() {
            let url = format!("{}/app/{}", BASE_URL, item.id);
            let currency = if item.price.currency.is_empty() {
                "USD".to_string()
            } else {
                item.price.currency.clone()
            };
            let price = (item.price.final_price as f64) / 100.0;

            let mut platforms = Vec::new();
            if item.platforms.windows {
                platforms.push("Windows");
            }
            if item.platforms.mac {
                platforms.push("macOS");
            }
            if item.platforms.linux {
                platforms.push("Linux");
            }

            let content = format!(
                "Price: {:.2} {} | Platforms: {}",
                price,
                currency,
                platforms.join(", ")
            );

            let mut result = SearchResult::new(item.name.clone(), url)
                .with_snippet(content.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Shopping);

            if !item.tiny_image.is_empty() {
                result = result.with_extra("thumbnail", serde_json::json!(item.tiny_image));
                result = result.with_extra("img_src", serde_json::json!(item.tiny_image));
            }
            result = result.with_extra("price", serde_json::json!(price));
            result = result.with_extra("currency", serde_json::json!(currency));
            result = result.with_extra("platforms", serde_json::json!(platforms.join(", ")));
            result = result.with_extra("app_id", serde_json::json!(item.id));

            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for SteamEngine {
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
        matches!(t, ResultType::Shopping | ResultType::Web | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), BASE_URL.into());
        s.insert("api_endpoint".into(), "/api/storesearch/".into());
        s.insert("results".into(), "JSON".into());
        s
    }
}

//! Chefkoch recipe search engine implementation.
//! German recipe database, JSON API.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Chefkoch (German recipe database) search engine.
pub struct ChefkochEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://api.chefkoch.de";
const THUMB_FORMAT: &str = "crop-240x300";

#[derive(Debug, Serialize, Deserialize)]
struct ChefkochResponse {
    #[serde(default)]
    results: Vec<ChefkochResult>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ChefkochResult {
    #[serde(default)]
    recipe: ChefkochRecipe,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ChefkochRecipe {
    #[serde(default)]
    title: String,
    #[serde(default)]
    subtitle: String,
    #[serde(default)]
    site_url: String,
    #[serde(default)]
    is_premium: bool,
    #[serde(default)]
    is_plus: bool,
    #[serde(default)]
    difficulty: serde_json::Value,
    #[serde(default)]
    preparation_time: serde_json::Value,
    #[serde(default)]
    ingredient_count: serde_json::Value,
    #[serde(default)]
    submission_date: String,
    #[serde(default)]
    preview_image_url_template: String,
}

impl ChefkochEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "chefkoch".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Chefkoch - German recipe database.".to_string(),
            website: Some("https://www.chefkoch.de".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Chefkoch HTTP client");
        ChefkochEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = format!("{}/v2/search-gateway/recipes", BASE_URL);
        let limit = "20".to_string();
        let offset = (query.offset * 20).to_string();

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("query", query.query.as_str()),
                ("limit", limit.as_str()),
                ("offset", offset.as_str()),
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
        let parsed: ChefkochResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        let mut idx = 0;
        for r in parsed.results.iter() {
            let recipe = &r.recipe;
            // skip_premium default true
            if recipe.is_premium || recipe.is_plus {
                continue;
            }
            if recipe.site_url.is_empty() {
                continue;
            }
            let difficulty = val_to_string(&recipe.difficulty);
            let prep = val_to_string(&recipe.preparation_time);
            let ingredient_count = val_to_string(&recipe.ingredient_count);
            let mut content_parts: Vec<String> = Vec::new();
            if !recipe.subtitle.is_empty() {
                content_parts.push(recipe.subtitle.clone());
            }
            content_parts.push(format!("Schwierigkeitsstufe (1-3): {}", difficulty));
            content_parts.push(format!("Zubereitungszeit: {}min", prep));
            content_parts.push(format!("Anzahl der Zutaten: {}", ingredient_count));

            let thumbnail = recipe
                .preview_image_url_template
                .replace("<format>", THUMB_FORMAT);
            let published = if recipe.submission_date.len() >= 19 {
                recipe.submission_date[..19].to_string()
            } else {
                recipe.submission_date.clone()
            };

            results.push(
                SearchResult::new(recipe.title.clone(), recipe.site_url.clone())
                    .with_snippet(content_parts.join(" | "))
                    .with_engine(self.name())
                    .with_rank(query.offset + idx + 1)
                    .with_score(1.0 - (idx as f64 * 0.05))
                    .with_result_type(ResultType::Web)
                    .with_extra("thumbnail", serde_json::json!(thumbnail))
                    .with_extra("published", serde_json::json!(published))
                    .with_extra("source", serde_json::json!("chefkoch")),
            );
            idx += 1;
        }
        Ok(results)
    }
}

fn val_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[async_trait]
impl Engine for ChefkochEngine {
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
        s.insert("base_url".into(), "https://api.chefkoch.de".into());
        s
    }
}

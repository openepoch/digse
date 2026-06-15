//! Material Icons search engine implementation
//!
//! fetches the Google Fonts
//! Material Symbols metadata and filters icon names/tags/categories
//! client-side. Category: images / icons.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Material Icons (Google Symbols) search engine
pub struct MaterialIconsEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const SEARCH_URL: &str =
    "https://fonts.google.com/metadata/icons?key=material_symbols&incomplete=true";

const RESULT_URL: &str = "https://fonts.google.com/icons?icon.query={query}&selected=Material+Symbols+Outlined:{icon_name}:FILL@0{fill};wght@400;GRAD@0;opsz@24";

const IMG_SRC_URL: &str = "https://fonts.gstatic.com/s/i/short-term/release/materialsymbolsoutlined/{icon_name}/{svg_type}/24px.svg";

#[derive(Debug, Serialize, Deserialize)]
struct MaterialMetadata {
    #[serde(default)]
    icons: Vec<MaterialIcon>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct MaterialIcon {
    #[serde(default)]
    name: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    categories: Vec<String>,
}

impl MaterialIconsEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "material_icons".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Material Symbols / Material Icons (Google Fonts).".to_string(),
            website: Some("https://fonts.google.com/icons".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Material Icons HTTP client");

        MaterialIconsEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() {
            return Ok(vec![]);
        }
        let response = self
            .client
            .get(SEARCH_URL)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
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

        // The response body starts with a 5-char guard (")]}'\n") before JSON.
        let json_text = if text.len() > 5 { &text[5..] } else { &text };
        let parsed: MaterialMetadata = match serde_json::from_str(json_text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let q_lower = query.query.to_lowercase();
        let outlined = !q_lower.contains("fill") && !q_lower.contains("filled");
        // strip the words "fill"/"filled" from the query (mirrors the regex)
        let stripped = q_lower
            .replace("filled", " ")
            .replace("fill", " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let svg_type = if outlined { "default" } else { "fill1" };
        let query_parts: Vec<String> = stripped
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        let mut results = Vec::new();
        let mut rank = 0usize;
        for icon in parsed.icons.iter() {
            if rank >= query.count {
                break;
            }
            let name_lower = icon.name.to_lowercase();
            let tags_lower: Vec<String> = icon.tags.iter().map(|t| t.to_lowercase()).collect();
            let cats_lower: Vec<String> =
                icon.categories.iter().map(|c| c.to_lowercase()).collect();

            let matched = query_parts.iter().any(|part| {
                name_lower.contains(part)
                    || tags_lower.iter().any(|t| t.contains(part))
                    || cats_lower.iter().any(|c| c.contains(part))
            });
            if !matched {
                continue;
            }
            rank += 1;
            let i = rank - 1;

            let display_name = icon.name.replace('_', "").to_title_case();
            let url = RESULT_URL
                .replace("{icon_name}", &icon.name)
                .replace("{query}", &icon.name)
                .replace("{fill}", if outlined { "0" } else { "1" });
            let img_src = IMG_SRC_URL
                .replace("{icon_name}", &icon.name)
                .replace("{svg_type}", svg_type);

            let tags_disp: Vec<String> = icon.tags.iter().map(|t| t.to_title_case()).collect();
            let cats_disp: Vec<String> = icon
                .categories
                .iter()
                .map(|c| c.to_title_case())
                .collect();
            let content = format!("{} / {}", tags_disp.join(", "), cats_disp.join(", "));

            let result = SearchResult::new(display_name, url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Images)
                .with_extra("img_src", serde_json::json!(img_src.clone()))
                .with_extra("thumbnail", serde_json::json!(img_src))
                .with_extra("format", serde_json::json!("SVG"))
                .with_extra("source", serde_json::json!("material_icons"));
            results.push(result);
        }
        Ok(results)
    }
}

// Minimal title-case helper to mirror Python's str.title() loosely.
trait TitleCase {
    fn to_title_case(&self) -> String;
}
impl TitleCase for str {
    fn to_title_case(&self) -> String {
        let mut out = String::with_capacity(self.len());
        let mut new_word = true;
        for ch in self.chars() {
            if ch.is_whitespace() || ch == '_' || ch == '-' {
                out.push(ch);
                new_word = true;
            } else if new_word {
                for u in ch.to_uppercase() {
                    out.push(u);
                }
                new_word = false;
            } else {
                for u in ch.to_lowercase() {
                    out.push(u);
                }
            }
        }
        out
    }
}

#[async_trait]
impl Engine for MaterialIconsEngine {
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
        matches!(result_type, ResultType::Images | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("metadata_url".to_string(), SEARCH_URL.to_string());
        settings.insert("website".to_string(), "https://fonts.google.com/icons".to_string());
        settings
    }
}

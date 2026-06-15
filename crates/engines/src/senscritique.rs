//! SensCritique search engine implementation
//!
//! SensCritique is a
//! French culture/movies review site exposing a GraphQL endpoint at
//! `https://apollo.senscritique.com/`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// SensCritique search engine (general / culture, GraphQL)
pub struct SensCritiqueEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl SensCritiqueEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "senscritique".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "SensCritique - French culture/movies review community.".to_string(),
            website: Some("https://www.senscritique.com/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create SensCritique HTTP client");
        SensCritiqueEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let graphql_url = "https://apollo.senscritique.com/";
        let page_size = 16;
        let offset = query.offset;
        let graphql_query = r#"query SearchProductExplorer($query: String, $offset: Int, $limit: Int,
                    $sortBy: SearchProductExplorerSort) {
  searchProductExplorer(
    query: $query
    filters: []
    sortBy: $sortBy
    offset: $offset
    limit: $limit
  ) {
    items {
      category
      dateRelease
      duration
      id
      originalTitle
      rating
      title
      url
      yearOfProduction
      medias {
        picture
      }
      countries {
        name
      }
      genresInfos {
        label
      }
      directors {
        name
      }
      stats {
        ratingCount
      }
    }
  }
}"#;
        let body = serde_json::json!({
            "operationName": "SearchProductExplorer",
            "variables": {
                "offset": offset,
                "limit": page_size,
                "query": query.query.as_str(),
                "sortBy": "RELEVANCE",
            },
            "query": graphql_query,
        });
        let resp = self
            .client
            .post(graphql_url)
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
        let text = resp
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        let root: ScResponse = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(_) => return Ok(vec![]),
        };
        let items = root
            .data
            .and_then(|d| d.search_product_explorer)
            .and_then(|s| s.items)
            .unwrap_or_default();
        let mut results = Vec::new();
        for item in items.iter() {
            let title = item.title.clone().unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let year = item.year_of_production.unwrap_or_default();
            let display_title = if year > 0 {
                format!("{} ({})", title, year)
            } else {
                title.clone()
            };
            let url = format!(
                "https://www.senscritique.com{}",
                item.url.clone().unwrap_or_default()
            );
            if url == "https://www.senscritique.com" {
                continue;
            }
            let thumbnail = item
                .medias
                .as_ref()
                .and_then(|m| m.picture.clone())
                .unwrap_or_default();
            let mut parts = Vec::new();
            if let Some(cat) = &item.category {
                parts.push(cat.clone());
            }
            if let Some(orig) = &item.original_title {
                if orig != &title {
                    parts.push(format!("Original title: {}", orig));
                }
            }
            if let Some(directors) = &item.directors {
                let names: Vec<String> = directors.iter().filter_map(|d| d.name.clone()).collect();
                if !names.is_empty() {
                    parts.push(format!("Director(s): {}", names.join(", ")));
                }
            }
            if let Some(countries) = &item.countries {
                let names: Vec<String> = countries.iter().filter_map(|c| c.name.clone()).collect();
                if !names.is_empty() {
                    parts.push(format!("Country: {}", names.join(", ")));
                }
            }
            if let Some(genres) = &item.genres_infos {
                let names: Vec<String> = genres.iter().filter_map(|g| g.label.clone()).collect();
                if !names.is_empty() {
                    parts.push(format!("Genre(s): {}", names.join(", ")));
                }
            }
            if let Some(dur) = item.duration {
                let minutes = dur / 60;
                if minutes > 0 {
                    parts.push(format!("Duration: {} min", minutes));
                }
            }
            if let (Some(rating), count) = (item.rating, item.stats.as_ref().and_then(|s| s.rating_count)) {
                if let Some(c) = count {
                    parts.push(format!("Rating: {}/10 ({} votes)", rating, c));
                }
            }
            results.push(
                SearchResult::new(display_title, url)
                    .with_snippet(parts.join(" | "))
                    .with_engine(self.name())
                    .with_result_type(ResultType::Web)
                    .with_extra("thumbnail", serde_json::json!(thumbnail))
                    .with_extra("source", serde_json::json!("senscritique")),
            );
        }
        Ok(results)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ScResponse {
    #[serde(default)]
    data: Option<ScData>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ScData {
    #[serde(default, rename = "searchProductExplorer")]
    search_product_explorer: Option<ScSearch>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ScSearch {
    #[serde(default)]
    items: Option<Vec<ScItem>>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ScItem {
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    duration: Option<i64>,
    #[serde(default)]
    original_title: Option<String>,
    #[serde(default)]
    rating: Option<f64>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    year_of_production: Option<i64>,
    #[serde(default)]
    medias: Option<ScMedias>,
    #[serde(default)]
    countries: Option<Vec<ScNamed>>,
    #[serde(default, rename = "genresInfos")]
    genres_infos: Option<Vec<ScNamed>>,
    #[serde(default)]
    directors: Option<Vec<ScNamed>>,
    #[serde(default)]
    stats: Option<ScStats>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ScMedias {
    #[serde(default)]
    picture: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ScNamed {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    label: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ScStats {
    #[serde(default)]
    rating_count: Option<i64>,
}

#[async_trait]
impl Engine for SensCritiqueEngine {
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
        let mut results = self.fetch_results(query).await?;
        for (i, r) in results.iter_mut().enumerate() {
            r.rank = query.offset + i + 1;
            r.score = 1.0 - (i as f64 * 0.05);
        }
        Ok(results)
    }
    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Web | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("graphql_url".into(), "https://apollo.senscritique.com/".into());
        s.insert("page_size".into(), "16".into());
        s
    }
}

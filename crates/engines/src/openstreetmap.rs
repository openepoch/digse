//! OpenStreetMap search engine implementation (map; JSON via Nominatim)
//!
//! Queries the Nominatim
//! geocoding API and returns map points with latitude/longitude/address.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// OpenStreetMap (Nominatim) search engine
pub struct OpenStreetMapEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct NominatimResult {
    #[serde(default)]
    display_name: String,
    #[serde(default, rename = "lat")]
    lat: String,
    #[serde(default, rename = "lon")]
    lon: String,
    #[serde(default)]
    boundingbox: Vec<String>,
    #[serde(default)]
    osm_type: Option<String>,
    #[serde(default)]
    osm_id: Option<i64>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default, rename = "type")]
    osm_category_type: Option<String>,
    #[serde(default)]
    address: Option<NominatimAddress>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct NominatimAddress {
    #[serde(default)]
    house_number: Option<String>,
    #[serde(default)]
    road: Option<String>,
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    town: Option<String>,
    #[serde(default)]
    village: Option<String>,
    #[serde(default)]
    postcode: Option<String>,
    #[serde(default)]
    country: Option<String>,
    #[serde(default)]
    country_code: Option<String>,
}

impl OpenStreetMapEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "openstreetmap".to_string(),
            category: EngineCategory::Maps,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "OpenStreetMap - Map search via Nominatim.".to_string(),
            website: Some("https://www.openstreetmap.org/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create OpenStreetMap HTTP client");

        OpenStreetMapEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://nominatim.openstreetmap.org/search";

        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("polygon_geojson", "1"),
                ("format", "jsonv2"),
                ("addressdetails", "1"),
                ("extratags", "1"),
                ("dedupe", "1"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: Vec<NominatimResult> = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, r) in parsed.iter().enumerate() {
            if r.display_name.is_empty() {
                continue;
            }
            let lat: f64 = r.lat.parse().unwrap_or(0.0);
            let lon: f64 = r.lon.parse().unwrap_or(0.0);

            let url = if let (Some(osm_type), Some(osm_id)) = (&r.osm_type, r.osm_id) {
                let t = match osm_type.as_str() {
                    "node" => "node",
                    "way" => "way",
                    "relation" => "relation",
                    _ => "node",
                };
                format!("https://openstreetmap.org/{}/{}", t, osm_id)
            } else {
                format!(
                    "https://www.openstreetmap.org/?mlat={}&mlon={}&zoom=12&layers=M",
                    lat, lon
                )
            };

            let address_str = self.format_address(r);

            results.push(
                SearchResult::new(&r.display_name, &url)
                    .with_snippet(address_str.clone())
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Maps)
                    .with_extra("latitude", serde_json::json!(lat))
                    .with_extra("longitude", serde_json::json!(lon))
                    .with_extra("address", serde_json::json!(address_str))
                    .with_extra("boundingbox", serde_json::json!(r.boundingbox))
                    .with_extra(
                        "category",
                        serde_json::json!(r.category.clone().unwrap_or_default()),
                    ),
            );
        }
        Ok(results)
    }

    fn format_address(&self, r: &NominatimResult) -> String {
        let mut parts = Vec::new();
        if let Some(addr) = &r.address {
            if let Some(n) = &addr.house_number {
                parts.push(n.clone());
            }
            if let Some(road) = &addr.road {
                parts.push(road.clone());
            }
            let city = addr
                .city
                .as_ref()
                .or(addr.town.as_ref())
                .or(addr.village.as_ref());
            if let Some(c) = city {
                parts.push(c.clone());
            }
            if let Some(pc) = &addr.postcode {
                parts.push(pc.clone());
            }
            if let Some(country) = &addr.country {
                parts.push(country.clone());
            }
        }
        if parts.is_empty() {
            r.display_name.clone()
        } else {
            parts.join(", ")
        }
    }
}

#[async_trait]
impl Engine for OpenStreetMapEngine {
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
        matches!(t, ResultType::Maps | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert(
            "base_url".to_string(),
            "https://nominatim.openstreetmap.org/".to_string(),
        );
        s
    }
}

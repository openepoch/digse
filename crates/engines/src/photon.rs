//! Photon search engine implementation (map; JSON)
//!
//! Photon is the Komoot geocoder,
//! powered by OpenStreetMap data. Returns map points with lat/lon/address.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Photon (Komoot) geocoder search engine
pub struct PhotonEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const PAGE_SIZE: i64 = 10;

#[derive(Debug, Serialize, Deserialize)]
struct PhotonResponse {
    #[serde(default)]
    features: Vec<PhotonFeature>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PhotonFeature {
    #[serde(default)]
    geometry: Option<PhotonGeometry>,
    #[serde(default)]
    properties: Option<PhotonProperties>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PhotonGeometry {
    #[serde(default)]
    coordinates: Vec<f64>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PhotonProperties {
    #[serde(default)]
    name: String,
    #[serde(default)]
    osm_type: String,
    #[serde(default)]
    osm_id: Option<i64>,
    #[serde(default)]
    osm_key: String,
    #[serde(default)]
    osm_value: String,
    #[serde(default)]
    housenumber: String,
    #[serde(default)]
    street: String,
    #[serde(default)]
    city: String,
    #[serde(default)]
    town: String,
    #[serde(default)]
    village: String,
    #[serde(default)]
    postcode: String,
    #[serde(default)]
    country: String,
    #[serde(default)]
    extent: Option<Vec<f64>>,
}

impl PhotonEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "photon".to_string(),
            category: EngineCategory::Maps,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Photon - Komoot geocoder (OpenStreetMap data).".to_string(),
            website: Some("https://photon.komoot.io".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Photon HTTP client");

        PhotonEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://photon.komoot.io/api/";
        let limit = PAGE_SIZE.to_string();

        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("limit", limit.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: PhotonResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, feature) in parsed.features.iter().enumerate() {
            let props = match &feature.properties {
                Some(p) => p,
                None => continue,
            };
            if props.name.is_empty() {
                continue;
            }
            let geometry = match &feature.geometry {
                Some(g) => g,
                None => continue,
            };
            if geometry.coordinates.len() < 2 {
                continue;
            }
            let lon = geometry.coordinates[0];
            let lat = geometry.coordinates[1];

            // osm-type mapping
            let osm_type = match props.osm_type.as_str() {
                "N" => "node",
                "W" => "way",
                "R" => "relation",
                _ => continue, // skip invalid osm-type
            };
            let osm_id = match props.osm_id {
                Some(id) => id,
                None => continue,
            };
            let url = format!("https://openstreetmap.org/{}/{}", osm_type, osm_id);

            let address_str = self.format_address(props);

            // bounding box from extent if present
            let boundingbox: Vec<f64> = match &props.extent {
                Some(ext) if ext.len() >= 4 => vec![ext[3], ext[1], ext[0], ext[2]],
                _ => vec![lat, lat, lon, lon],
            };

            results.push(
                SearchResult::new(&props.name, &url)
                    .with_snippet(address_str.clone())
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Maps)
                    .with_extra("latitude", serde_json::json!(lat))
                    .with_extra("longitude", serde_json::json!(lon))
                    .with_extra("address", serde_json::json!(address_str))
                    .with_extra("boundingbox", serde_json::json!(boundingbox))
                    .with_extra(
                        "category",
                        serde_json::json!(format!("{}: {}", props.osm_key, props.osm_value)),
                    ),
            );
        }
        Ok(results)
    }

    fn format_address(&self, props: &PhotonProperties) -> String {
        let mut parts = Vec::new();
        // only build a structured address when this is a named amenity/shop/etc.
        let is_named = matches!(
            props.osm_key.as_str(),
            "amenity" | "shop" | "tourism" | "leisure"
        );
        if is_named {
            if !props.housenumber.is_empty() {
                parts.push(props.housenumber.clone());
            }
            if !props.street.is_empty() {
                parts.push(props.street.clone());
            }
            let city = if !props.city.is_empty() {
                &props.city
            } else if !props.town.is_empty() {
                &props.town
            } else {
                &props.village
            };
            if !city.is_empty() {
                parts.push(city.clone());
            }
            if !props.postcode.is_empty() {
                parts.push(props.postcode.clone());
            }
            if !props.country.is_empty() {
                parts.push(props.country.clone());
            }
        }
        if parts.is_empty() {
            // fall back to a display string
            let mut s = vec![props.name.clone()];
            if !props.city.is_empty() {
                s.push(props.city.clone());
            } else if !props.town.is_empty() {
                s.push(props.town.clone());
            } else if !props.village.is_empty() {
                s.push(props.village.clone());
            }
            if !props.country.is_empty() {
                s.push(props.country.clone());
            }
            s.join(", ")
        } else {
            parts.join(", ")
        }
    }
}

#[async_trait]
impl Engine for PhotonEngine {
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
        s.insert("base_url".to_string(), "https://photon.komoot.io/".to_string());
        s.insert("page_size".to_string(), PAGE_SIZE.to_string());
        s
    }
}

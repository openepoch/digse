//! Apple Maps search engine implementation (MapKit JSON API)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Apple Maps (MapKit) search engine
pub struct AppleMapsEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    token: tokio::sync::Mutex<Option<String>>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct MapKitResponse {
    #[serde(default)]
    results: Vec<MapKitPlace>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct MapKitPlace {
    #[serde(default)]
    name: String,
    #[serde(default, rename = "placecardUrl")]
    placecard_url: String,
    #[serde(default)]
    center: MapKitCenter,
    #[serde(default, rename = "displayMapRegion")]
    display_map_region: Option<MapKitRegion>,
    #[serde(default)]
    telephone: String,
    #[serde(default)]
    urls: Vec<String>,
    #[serde(default)]
    locality: String,
    #[serde(default)]
    country: String,
    #[serde(default, rename = "postCode")]
    post_code: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct MapKitCenter {
    #[serde(default)]
    lat: f64,
    #[serde(default)]
    lng: f64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct MapKitRegion {
    #[serde(default, rename = "southLat")]
    south_lat: f64,
    #[serde(default, rename = "northLat")]
    north_lat: f64,
    #[serde(default, rename = "westLng")]
    west_lng: f64,
    #[serde(default, rename = "eastLng")]
    east_lng: f64,
}

impl AppleMapsEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "apple_maps".to_string(),
            category: EngineCategory::Maps,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Apple Maps - places search (MapKit API).".to_string(),
            website: Some("https://www.apple.com/maps/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Apple Maps HTTP client");

        AppleMapsEngine {
            metadata,
            client,
            token: tokio::sync::Mutex::new(None),
        }
    }

    /// Obtain a MapKit access token via DuckDuckGo's bootstrap endpoint.
    /// Returns None on any failure (graceful degradation).
    async fn obtain_token(&self) -> Option<String> {
        // Step 1: get a temp token from duckduckgo
        let tmp = self.client
            .get("https://duckduckgo.com/local.js?get_mk_token=1")
            .header("User-Agent", "digse/0.1.0")
            .send()
            .await
            .ok()?
            .text()
            .await
            .ok()?;

        // Step 2: exchange it for a real access token
        let resp = self.client
            .get("https://cdn.apple-mapkit.com/ma/bootstrap?apiVersion=2&mkjsVersion=5.72.53&poi=1")
            .header("User-Agent", "digse/0.1.0")
            .header("Authorization", format!("Bearer {}", tmp.trim()))
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let text = resp.text().await.ok()?;
        let v: serde_json::Value = serde_json::from_str(&text).ok()?;
        v.get("authInfo")
            .and_then(|a| a.get("access_token"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
    }

    async fn get_token(&self) -> Option<String> {
        let mut guard = self.token.lock().await;
        if let Some(t) = guard.as_ref() {
            return Some(t.clone());
        }
        let t = self.obtain_token().await?;
        *guard = Some(t.clone());
        Some(t)
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let token = match self.get_token().await {
            Some(t) => t,
            None => {
                eprintln!("apple_maps: could not obtain MapKit token");
                return Ok(vec![]);
            }
        };

        let resp = self.client
            .get("https://api.apple-mapkit.com/v1/search")
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .header("Authorization", format!("Bearer {}", token))
            .query(&[
                ("q", query.query.as_str()),
                ("lang", "en"),
                ("mkjsVersion", "5.72.53"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: MapKitResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, place) in parsed.results.iter().enumerate() {
            if place.name.is_empty() {
                continue;
            }
            let url = if place.placecard_url.is_empty() {
                format!(
                    "https://maps.apple.com/?ll={},{}&q={}",
                    place.center.lat,
                    place.center.lng,
                    urlencoding::encode(&place.name)
                )
            } else {
                place.placecard_url.clone()
            };
            let address = vec![
                place.locality.clone(),
                place.post_code.clone(),
                place.country.clone(),
            ]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ");

            let mut r = SearchResult::new(place.name.clone(), url)
                .with_snippet(address.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Maps)
                .with_extra("latitude", serde_json::json!(place.center.lat))
                .with_extra("longitude", serde_json::json!(place.center.lng))
                .with_extra("address", serde_json::json!(address));

            if let Some(region) = &place.display_map_region {
                r = r.with_extra(
                    "boundingbox",
                    serde_json::json!([
                        region.south_lat,
                        region.north_lat,
                        region.west_lng,
                        region.east_lng,
                    ]),
                );
            }
            results.push(r);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for AppleMapsEngine {
    fn name(&self) -> &str { &self.metadata.name }
    fn category(&self) -> EngineCategory { self.metadata.category }
    fn is_enabled(&self) -> bool { self.metadata.enabled }
    fn metadata(&self) -> EngineMetadata { self.metadata.clone() }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        self.fetch_results(query).await
    }

    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Maps | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://api.apple-mapkit.com".to_string());
        s
    }
}

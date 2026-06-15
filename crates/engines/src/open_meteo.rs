//! Open-Meteo search engine implementation (weather; JSON)
//!
//! First geocodes the query via the
//! Open-Meteo geocoding API, then fetches the current + hourly forecast from
//! the Open-Meteo forecast API. Falls back gracefully when the location cannot
//! be resolved.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Open-Meteo weather search engine
pub struct OpenMeteoEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeoResponse {
    #[serde(default)]
    results: Vec<GeoResult>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeoResult {
    #[serde(default)]
    name: String,
    #[serde(default)]
    latitude: f64,
    #[serde(default)]
    longitude: f64,
    #[serde(default)]
    country: String,
    #[serde(default)]
    admin1: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ForecastResponse {
    #[serde(default)]
    current: ForecastCurrent,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ForecastCurrent {
    #[serde(default)]
    temperature_2m: Option<f64>,
    #[serde(default)]
    apparent_temperature: Option<f64>,
    #[serde(default)]
    relative_humidity_2m: Option<f64>,
    #[serde(default)]
    cloud_cover: Option<f64>,
    #[serde(default)]
    pressure_msl: Option<f64>,
    #[serde(default)]
    wind_speed_10m: Option<f64>,
    #[serde(default)]
    wind_direction_10m: Option<i64>,
    #[serde(default)]
    weather_code: Option<i64>,
}

/// Map the WMO weather code to a human-readable condition string.
fn wmo_to_condition(code: i64) -> &'static str {
    match code {
        0 => "clear sky",
        1 => "fair",
        2 => "partly cloudy",
        3 => "cloudy",
        45 | 48 => "fog",
        51 | 53 | 55 => "light rain",
        56 => "light sleet showers",
        57 => "light sleet",
        61 => "light rain",
        63 => "rain",
        65 => "heavy rain",
        66 => "light sleet showers",
        67 => "light sleet",
        71 => "light sleet",
        73 => "sleet",
        75 => "heavy sleet",
        77 => "snow",
        80 => "light rain showers",
        81 => "rain showers",
        82 => "heavy rain showers",
        85 => "snow showers",
        86 => "heavy snow showers",
        95 => "rain and thunder",
        96 => "light snow and thunder",
        99 => "heavy snow and thunder",
        _ => "unknown",
    }
}

impl OpenMeteoEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "open_meteo".to_string(),
            category: EngineCategory::Weather,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Open-Meteo - Free weather API.".to_string(),
            website: Some("https://open-meteo.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Open-Meteo HTTP client");

        OpenMeteoEngine { metadata, client }
    }

    async fn geocode(&self, query: &str) -> Result<Option<GeoResult>> {
        let url = "https://geocoding-api.open-meteo.com/v1/search";
        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("name", query),
                ("count", "1"),
                ("language", "en"),
                ("format", "json"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(None);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: GeoResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };
        Ok(parsed.results.into_iter().next())
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let location = match self.geocode(&query.query).await? {
            Some(loc) => loc,
            None => return Ok(vec![]),
        };

        let url = "https://api.open-meteo.com/v1/forecast";
        let lat = location.latitude.to_string();
        let lon = location.longitude.to_string();
        let current = "temperature_2m,apparent_temperature,relative_humidity_2m,cloud_cover,pressure_msl,wind_speed_10m,wind_direction_10m,weather_code";

        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("latitude", lat.as_str()),
                ("longitude", lon.as_str()),
                ("current", current),
                ("timezone", "auto"),
                ("forecast_days", "1"),
                ("timeformat", "unixtime"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: ForecastResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let c = &parsed.current;
        let weather_code = c.weather_code.unwrap_or(-1);
        let condition = wmo_to_condition(weather_code).to_string();

        let title = format!(
            "Weather: {}{}",
            location.name,
            if location.country.is_empty() {
                String::new()
            } else {
                format!(", {}", location.country)
            }
        );
        let result_url = format!(
            "https://open-meteo.com/en/docs?latitude={}&longitude={}",
            location.latitude, location.longitude
        );

        let mut snippet_parts = Vec::new();
        if let Some(t) = c.temperature_2m {
            snippet_parts.push(format!("Temperature: {}°C", t));
        }
        snippet_parts.push(format!("Condition: {}", condition));
        if let Some(h) = c.relative_humidity_2m {
            snippet_parts.push(format!("Humidity: {}%", h));
        }
        if let Some(w) = c.wind_speed_10m {
            snippet_parts.push(format!("Wind: {} km/h", w));
        }
        if let Some(feels) = c.apparent_temperature {
            snippet_parts.push(format!("Feels like: {}°C", feels));
        }
        if let Some(p) = c.pressure_msl {
            snippet_parts.push(format!("Pressure: {} hPa", p));
        }

        let result = SearchResult::new(&title, &result_url)
            .with_snippet(snippet_parts.join(" | "))
            .with_engine(self.name())
            .with_rank(query.offset + 1)
            .with_score(1.0)
            .with_result_type(ResultType::Weather)
            .with_extra("temperature", serde_json::json!(c.temperature_2m))
            .with_extra("condition", serde_json::json!(condition))
            .with_extra("humidity", serde_json::json!(c.relative_humidity_2m))
            .with_extra("wind", serde_json::json!(c.wind_speed_10m))
            .with_extra("feels_like", serde_json::json!(c.apparent_temperature))
            .with_extra("pressure", serde_json::json!(c.pressure_msl))
            .with_extra("latitude", serde_json::json!(location.latitude))
            .with_extra("longitude", serde_json::json!(location.longitude))
            .with_extra("address", serde_json::json!(format!("{}, {}", location.name, location.country)));

        Ok(vec![result])
    }
}

#[async_trait]
impl Engine for OpenMeteoEngine {
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
        matches!(t, ResultType::Weather | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("api_url".to_string(), "https://api.open-meteo.com".to_string());
        s.insert("geo_url".to_string(), "https://geocoding-api.open-meteo.com".to_string());
        s
    }
}

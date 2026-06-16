//! wttr.in weather forecast search engine implementation (JSON).

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// wttr.in weather forecast engine.
pub struct WttrEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl WttrEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "wttr".to_string(),
            category: EngineCategory::Weather,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "wttr.in - console weather forecast service.".to_string(),
            website: Some("https://wttr.in".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create wttr.in HTTP client");
        WttrEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let location = query.query.trim();
        if location.is_empty() {
            return Ok(vec![]);
        }
        let lang = query.language.clone().unwrap_or_else(|| "en".to_string());
        let lang = lang.split('-').next().unwrap_or("en");
        let url = format!(
            "https://wttr.in/{}?format=j1&lang={}",
            urlencoding::encode(location),
            lang
        );

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        // upstream: 404 means no such location -> empty
        if resp.status().as_u16() == 404 || !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: WttrResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let cur = match parsed.current_condition.first() {
            Some(c) => c,
            None => return Ok(vec![]),
        };
        let condition = wwo_condition(&cur.weather_code);
        let title = format!("Weather: {} ({})", location, condition);

        let snippet = format!(
            "{}°C (feels {}°C) | {} | humidity {}% | wind {} km/h {} | cloud {}%",
            cur.temp_c, cur.feels_like_c, condition, cur.humidity, cur.wind_kmph,
            compass(&cur.wind_dir_degree), cur.cloudcover
        );

        let result_url = format!("https://wttr.in/{}", urlencoding::encode(location));
        let r = SearchResult::new(title, result_url)
            .with_snippet(snippet)
            .with_engine(self.name())
            .with_rank(1)
            .with_score(1.0)
            .with_result_type(ResultType::Weather)
            .with_extra("temperature", serde_json::json!(cur.temp_c))
            .with_extra("condition", serde_json::json!(condition))
            .with_extra("feels_like", serde_json::json!(cur.feels_like_c))
            .with_extra("humidity", serde_json::json!(cur.humidity))
            .with_extra("wind_speed", serde_json::json!(cur.wind_kmph))
            .with_extra("wind_from", serde_json::json!(compass(&cur.wind_dir_degree)))
            .with_extra("cloud_cover", serde_json::json!(cur.cloudcover));
        Ok(vec![r])
    }
}

#[derive(Debug, Deserialize)]
struct WttrResponse {
    #[serde(default)]
    current_condition: Vec<WttrCurrent>,
}

#[derive(Debug, Deserialize)]
struct WttrCurrent {
    #[serde(default)]
    #[serde(rename = "temp_C")]
    temp_c: String,
    #[serde(default)]
    #[serde(rename = "FeelsLikeC")]
    feels_like_c: String,
    #[serde(default)]
    #[serde(rename = "weatherCode")]
    weather_code: String,
    #[serde(default)]
    humidity: String,
    #[serde(default)]
    #[serde(rename = "windspeedKmph")]
    wind_kmph: String,
    #[serde(default)]
    #[serde(rename = "winddirDegree")]
    wind_dir_degree: String,
    #[serde(default)]
    cloudcover: String,
}

/// Map a WWO weather code to a human-readable condition (subset from upstream).
fn wwo_condition(code: &str) -> &'static str {
    match code {
        "113" => "clear sky",
        "116" => "partly cloudy",
        "119" => "cloudy",
        "122" => "fair",
        "143" => "fair",
        "176" => "light rain showers",
        "179" => "light snow showers",
        "182" => "light sleet showers",
        "185" => "light sleet",
        "200" => "rain and thunder",
        "227" => "light snow",
        "230" => "heavy snow",
        "248" | "260" => "fog",
        "263" | "266" => "light rain showers",
        "281" => "light sleet showers",
        "284" => "light snow showers",
        "293" => "light rain showers",
        "296" => "light rain",
        "299" => "rain showers",
        "302" => "rain",
        "305" => "heavy rain showers",
        "308" => "heavy rain",
        "311" => "light sleet",
        "314" => "sleet",
        "317" => "light sleet",
        "320" => "heavy sleet",
        "323" | "326" | "368" => "light snow showers",
        "329" | "332" | "338" => "heavy snow",
        "335" | "371" => "heavy snow showers",
        "350" => "light sleet",
        "353" => "light rain showers",
        "356" => "heavy rain showers",
        "359" => "heavy rain",
        "362" => "light sleet showers",
        "365" => "sleet showers",
        "374" => "light sleet showers",
        "377" => "heavy sleet",
        "386" => "rain showers and thunder",
        "389" => "heavy rain showers and thunder",
        "392" => "snow showers and thunder",
        "395" => "heavy snow showers",
        _ => "unknown",
    }
}

/// Convert a wind direction in degrees to a 16-point compass label.
fn compass(deg_str: &str) -> String {
    let deg: f64 = deg_str.parse().unwrap_or(0.0);
    let dirs = [
        "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW",
        "NW", "NNW",
    ];
    let idx = ((deg + 11.25) / 22.5).floor() as usize % 16;
    dirs[idx].to_string()
}

#[async_trait]
impl Engine for WttrEngine {
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
        s.insert("base_url".to_string(), "https://wttr.in".to_string());
        s.insert("results".to_string(), "JSON".to_string());
        s.insert("format".to_string(), "j1".to_string());
        s
    }
}

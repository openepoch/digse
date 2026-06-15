//! DuckDuckGo Weather engine implementation
//!
//! queries DDG's spice forecast
//! endpoint and surfaces current weather for a location.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// DuckDuckGo weather engine
pub struct DuckDuckGoWeatherEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct WeatherForecast {
    #[serde(default)]
    currentWeather: CurrentWeather,
    #[serde(default)]
    forecastHourly: ForecastHourly,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct CurrentWeather {
    #[serde(default)]
    temperature: f64,
    #[serde(default)]
    conditionCode: String,
    #[serde(default)]
    temperatureApparent: f64,
    #[serde(default)]
    windDirection: f64,
    #[serde(default)]
    windSpeed: f64,
    #[serde(default)]
    pressure: f64,
    #[serde(default)]
    humidity: f64,
    #[serde(default)]
    cloudCover: f64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ForecastHourly {
    #[serde(default)]
    hours: Vec<CurrentWeather>,
}

impl DuckDuckGoWeatherEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "duckduckgo_weather".to_string(),
            category: EngineCategory::Weather,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "DuckDuckGo weather forecast.".to_string(),
            website: Some("https://duckduckgo.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create DDG weather HTTP client");

        DuckDuckGoWeatherEngine { metadata, client }
    }

    /// Map a WeatherKit condition code to a human-readable condition.
    fn map_condition(code: &str) -> String {
        match code {
            "BlowingDust" => "fog",
            "Clear" | "MostlyClear" => "clear sky",
            "Cloudy" => "cloudy",
            "Foggy" | "Haze" | "Smoky" => "fog",
            "MostlyCloudy" | "PartlyCloudy" => "partly cloudy",
            "Breezy" | "Windy" => "partly cloudy",
            "Drizzle" => "light rain",
            "HeavyRain" => "heavy rain",
            "IsolatedThunderstorms" => "rain and thunder",
            "Rain" | "SunShowers" => "rain",
            "ScatteredThunderstorms" | "StrongStorms" => "heavy rain and thunder",
            "Thunderstorms" => "rain and thunder",
            "Frigid" | "Hot" => "clear sky",
            "Hail" => "heavy rain",
            "Flurries" | "SunFlurries" | "Snow" => "light snow",
            "Sleet" | "WintryMix" => "sleet",
            "Blizzard" | "BlowingSnow" | "HeavySnow" => "heavy snow",
            "FreezingDrizzle" => "light sleet",
            "FreezingRain" => "sleet",
            "Hurricane" | "TropicalStorm" => "rain and thunder",
            _ => "unknown",
        }
        .to_string()
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let location = query.query.trim();
        if location.is_empty() {
            return Ok(vec![]);
        }
        let encoded = urlencoding::encode(location);
        let lang = query
            .language
            .as_deref()
            .unwrap_or("en")
            .split('-')
            .next()
            .unwrap_or("en");
        let url = format!(
            "https://duckduckgo.com/js/spice/forecast/{}/{}",
            encoded, lang
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
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

        if text.trim() == "ddg_spice_forecast();" {
            return Ok(vec![]);
        }

        // DDG spice responses are wrapped in a JS callback: trim first/last lines
        let json_str: String = if let Some(first_nl) = text.find('\n') {
            let rest = &text[first_nl + 1..];
            if let Some(last_nl) = rest.rfind('\n') {
                rest[..last_nl.saturating_sub(1)].to_string()
            } else {
                rest.to_string()
            }
        } else {
            text.clone()
        };

        let parsed: WeatherForecast = match serde_json::from_str(&json_str) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let cur = parsed.currentWeather;
        let condition = Self::map_condition(&cur.conditionCode);
        let answer = format!(
            "Weather for {}: {} °C, {} (feels like {} °C, humidity {:.0}%, wind {:.1} mi/h)",
            location,
            cur.temperature,
            condition,
            cur.temperatureApparent,
            cur.humidity * 100.0,
            cur.windSpeed
        );
        let search_url = format!("https://duckduckgo.com/?q={}+weather", encoded);

        let result = SearchResult::new(format!("Weather: {}", location), search_url)
            .with_snippet(answer.clone())
            .with_engine(self.name())
            .with_rank(query.offset + 1)
            .with_score(1.0)
            .with_result_type(ResultType::Weather)
            .with_extra("location", serde_json::json!(location))
            .with_extra("temperature", serde_json::json!(cur.temperature))
            .with_extra("condition", serde_json::json!(condition))
            .with_extra("feels_like", serde_json::json!(cur.temperatureApparent))
            .with_extra("humidity", serde_json::json!(cur.humidity * 100.0))
            .with_extra("wind_speed", serde_json::json!(cur.windSpeed))
            .with_extra("wind_direction", serde_json::json!(cur.windDirection))
            .with_extra("pressure", serde_json::json!(cur.pressure))
            .with_extra("cloud_cover", serde_json::json!(cur.cloudCover * 100.0));

        Ok(vec![result])
    }
}

#[async_trait]
impl Engine for DuckDuckGoWeatherEngine {
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
        matches!(result_type, ResultType::Weather | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert(
            "base_url".to_string(),
            "https://duckduckgo.com/js/spice/forecast".to_string(),
        );
        settings
    }
}

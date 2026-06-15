//! Currency convert (DuckDuckGo) search engine implementation
//!
//! DuckDuckGo currency conversion. Queries the DDG spice currency endpoint and
//! returns a single Answer-style result describing the conversion rate.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// DuckDuckGo currency conversion engine
pub struct CurrencyConvertEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct CurrencyResponse {
    #[serde(default)]
    to: Vec<CurrencyConversion>,
    #[serde(default)]
    from: String,
    #[serde(default)]
    amount: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct CurrencyConversion {
    #[serde(default)]
    mid: f64,
    #[serde(default)]
    #[serde(rename = "to-amount")]
    to_amount: f64,
    #[serde(default)]
    #[serde(rename = "to-symbol")]
    to_symbol: String,
}

impl CurrencyConvertEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "currency_convert".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "DuckDuckGo currency conversion.".to_string(),
            website: Some("https://duckduckgo.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create currency_convert HTTP client");

        CurrencyConvertEngine { metadata, client }
    }

    /// Parse the user query for an amount and currency codes.
    /// e.g. "100 usd to eur" -> (100.0, "usd", "eur")
    fn parse_query(&self, q: &str) -> Option<(f64, String, String)> {
        let lower = q.to_lowercase();
        let tokens: Vec<&str> = lower
            .split_whitespace()
            .filter(|t| !t.is_empty())
            .collect();
        if tokens.is_empty() {
            return None;
        }
        // amount is the first parseable number, or defaults to 1
        let amount = tokens[0].parse::<f64>().unwrap_or(1.0);
        // find the "to" keyword (or default: treat last two currency tokens)
        let mut from_cur = String::new();
        let mut to_cur = String::new();
        let mut idx_to: Option<usize> = None;
        for (i, t) in tokens.iter().enumerate() {
            if *t == "to" || *t == "in" || *t == "->" {
                idx_to = Some(i);
                break;
            }
        }
        if let Some(idx) = idx_to {
            // from currency: tokens before idx that look like currency codes
            for t in &tokens[1..idx] {
                let c = t.trim_matches(|c: char| !c.is_alphanumeric());
                if c.len() == 3 && c.chars().all(|c| c.is_alphabetic()) {
                    from_cur = c.to_string();
                    break;
                }
            }
            // to currency: tokens after idx
            if idx + 1 < tokens.len() {
                let c = tokens[idx + 1].trim_matches(|c: char| !c.is_alphanumeric());
                if c.len() == 3 && c.chars().all(|c| c.is_alphabetic()) {
                    to_cur = c.to_string();
                }
            }
        } else {
            // No "to": try to extract two 3-letter codes
            let mut codes: Vec<String> = Vec::new();
            for t in &tokens {
                let c = t.trim_matches(|c: char| !c.is_alphanumeric());
                if c.len() == 3 && c.chars().all(|c| c.is_alphabetic()) {
                    codes.push(c.to_string());
                }
            }
            if codes.len() >= 2 {
                from_cur = codes[0].clone();
                to_cur = codes[1].clone();
            }
        }
        if from_cur.is_empty() || to_cur.is_empty() {
            return None;
        }
        Some((amount, from_cur, to_cur))
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let (amount, from, to) = match self.parse_query(&query.query) {
            Some(v) => v,
            None => return Ok(vec![]),
        };

        let amount_str = (1.0_f64).to_string();
        let url = format!(
            "https://duckduckgo.com/js/spice/currency/{}/{}",
            amount_str, to
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("from", from.as_str()),
                ("to", to.as_str()),
                ("amount", amount_str.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let text = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;

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

        let parsed: CurrencyResponse = match serde_json::from_str(&json_str) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        if parsed.to.is_empty() {
            return Ok(vec![]);
        }
        let rate = parsed.to[0].mid;
        if rate <= 0.0 {
            return Ok(vec![]);
        }

        let converted = amount * rate;
        let answer = format!(
            "{} {} = {:.4} {} (1 {} : {} {})",
            amount, from, converted, to, from, rate, to
        );
        let ddg_link = format!("https://duckduckgo.com/?q={}+to+{}", from, to);

        let result = SearchResult::new(answer.clone(), ddg_link.clone())
            .with_snippet(format!("Conversion rate: 1 {} = {} {}", from, rate, to))
            .with_engine(self.name())
            .with_rank(query.offset + 1)
            .with_score(1.0)
            .with_result_type(ResultType::Web)
            .with_extra("from_currency", serde_json::json!(from))
            .with_extra("to_currency", serde_json::json!(to))
            .with_extra("amount", serde_json::json!(amount))
            .with_extra("rate", serde_json::json!(rate))
            .with_extra("converted", serde_json::json!(converted));

        Ok(vec![result])
    }
}

#[async_trait]
impl Engine for CurrencyConvertEngine {
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
        matches!(result_type, ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert(
            "base_url".to_string(),
            "https://duckduckgo.com/js/spice/currency".to_string(),
        );
        settings.insert("engine_type".to_string(), "online_currency".to_string());
        settings
    }
}

//! Cloudflare AI engine implementation.
//! Paid AI inference via Cloudflare AI Gateway.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Cloudflare AI (Workers AI) engine - paid, requires gateway + API key + model.
pub struct CloudflareaiEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    api_key: Option<String>,
    account_id: Option<String>,
    gateway: Option<String>,
    model: Option<String>,
}

#[derive(Debug, Serialize)]
struct CfMessage {
    role: &'static str,
    content: String,
}

#[derive(Debug, Serialize)]
struct CfRequest {
    messages: Vec<CfMessage>,
}

#[derive(Debug, Deserialize)]
struct CfResponse {
    #[serde(default)]
    result: Option<CfResult>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct CfResult {
    #[serde(default)]
    response: String,
}

impl CloudflareaiEngine {
    pub fn new() -> Self {
        let api_key = std::env::var("CLOUDFLAREAI_API_KEY")
            .ok()
            .filter(|s| !s.is_empty());
        let account_id = std::env::var("CLOUDFLAREAI_ACCOUNT_ID")
            .ok()
            .filter(|s| !s.is_empty());
        let gateway = std::env::var("CLOUDFLAREAI_GATEWAY")
            .ok()
            .filter(|s| !s.is_empty());
        let model = std::env::var("CLOUDFLAREAI_MODEL")
            .ok()
            .filter(|s| !s.is_empty());
        let has_all = api_key.is_some() && account_id.is_some() && gateway.is_some() && model.is_some();
        let metadata = EngineMetadata {
            name: "cloudflareai".to_string(),
            category: EngineCategory::General,
            enabled: has_all,
            requires_auth: true,
            timeout_seconds: 30,
            description: "Cloudflare AI - Workers AI inference via AI Gateway.".to_string(),
            website: Some("https://ai.cloudflare.com".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create Cloudflare AI HTTP client");
        CloudflareaiEngine {
            metadata,
            client,
            api_key,
            account_id,
            gateway,
            model,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let (api_key, account_id, gateway, model) =
            match (&self.api_key, &self.account_id, &self.gateway, &self.model) {
                (Some(k), Some(a), Some(g), Some(m)) => (k, a, g, m),
                _ => {
                    tracing::warn!(
                        "cloudflareai requires CLOUDFLAREAI_API_KEY, _ACCOUNT_ID, _GATEWAY, _MODEL"
                    );
                    return Ok(vec![]);
                }
            };
        let url = format!(
            "https://gateway.ai.cloudflare.com/v1/{acct}/{gw}/workers-ai/{model}",
            acct = account_id,
            gw = gateway,
            model = model,
        );
        let body = CfRequest {
            messages: vec![
                CfMessage {
                    role: "assistant",
                    content: "Keep your answers as short and effective as possible.".to_string(),
                },
                CfMessage {
                    role: "system",
                    content:
                        "You are a helpful assistant. Be honest and direct about any question."
                            .to_string(),
                },
                CfMessage {
                    role: "user",
                    content: query.query.clone(),
                },
            ],
        };

        let resp = self
            .client
            .post(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
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
        let parsed: CfResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };
        if let Some(err) = parsed.error {
            tracing::warn!("cloudflareai error: {}", err);
            return Ok(vec![]);
        }
        let response = parsed.result.map(|r| r.response).unwrap_or_default();
        if response.is_empty() {
            return Ok(vec![]);
        }
        // AI answer rendered as a single infobox-style result.
        Ok(vec![SearchResult::new(
            "Cloudflare AI".to_string(),
            "https://ai.cloudflare.com".to_string(),
        )
        .with_snippet(response)
        .with_engine(self.name())
        .with_rank(1)
        .with_score(1.0)
        .with_result_type(ResultType::Web)
        .with_extra("infobox", serde_json::json!("Cloudflare AI"))])
    }
}

#[async_trait]
impl Engine for CloudflareaiEngine {
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
        s.insert(
            "endpoint".into(),
            "https://gateway.ai.cloudflare.com/v1".into(),
        );
        s
    }
}

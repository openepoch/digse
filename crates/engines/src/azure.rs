//! Azure resources search engine implementation
//! (paid; requires AZURE_TENANT_ID / AZURE_CLIENT_ID / AZURE_CLIENT_SECRET)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Azure Portal resource-group/resource search engine
pub struct AzureEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    tenant_id: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AzureBatchResponse {
    #[serde(default)]
    responses: Vec<AzureBatchItem>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AzureBatchItem {
    #[serde(default)]
    name: String,
    #[serde(default)]
    content: AzureBatchContent,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct AzureBatchContent {
    #[serde(default)]
    data: Vec<AzureResource>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct AzureResource {
    #[serde(default)]
    name: String,
    #[serde(default)]
    #[serde(rename = "subscriptionId")]
    subscription_id: String,
    #[serde(default, rename = "resourceGroup")]
    resource_group: String,
    #[serde(default, rename = "type")]
    resource_type: String,
}

impl AzureEngine {
    pub fn new() -> Self {
        let tenant_id = std::env::var("AZURE_TENANT_ID").ok().filter(|s| !s.is_empty());
        let client_id = std::env::var("AZURE_CLIENT_ID").ok().filter(|s| !s.is_empty());
        let client_secret = std::env::var("AZURE_CLIENT_SECRET").ok().filter(|s| !s.is_empty());

        let metadata = EngineMetadata {
            name: "azure".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: true,
            timeout_seconds: 20,
            description: "Azure - search Azure Portal resources & resource groups.".to_string(),
            website: Some("https://www.portal.azure.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("Failed to create Azure HTTP client");

        AzureEngine { metadata, client, tenant_id, client_id, client_secret }
    }

    async fn authenticate(&self) -> Option<String> {
        let (tenant, cid, secret) = match (&self.tenant_id, &self.client_id, &self.client_secret) {
            (Some(t), Some(c), Some(s)) => (t.clone(), c.clone(), s.clone()),
            _ => {
                eprintln!("azure requires AZURE_TENANT_ID, AZURE_CLIENT_ID and AZURE_CLIENT_SECRET");
                return None;
            }
        };
        let url = format!("https://login.microsoftonline.com/{}/oauth2/v2.0/token", tenant);
        let form = vec![
            ("client_id", cid),
            ("client_secret", secret),
            ("grant_type", "client_credentials".to_string()),
            ("scope", "https://management.azure.com/.default".to_string()),
        ];

        let resp = self.client
            .post(&url)
            .form(&form)
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let text = resp.text().await.ok()?;
        let v: serde_json::Value = serde_json::from_str(&text).ok()?;
        v.get("access_token").and_then(|t| t.as_str()).map(|s| s.to_string())
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let token = match self.authenticate().await {
            Some(t) => t,
            None => return Ok(vec![]),
        };

        let endpoint = "https://management.azure.com/batch?api-version=2020-06-01";
        let q = &query.query;
        let body = serde_json::json!({
            "requests": [
                {
                    "url": "/providers/Microsoft.ResourceGraph/resources?api-version=2024-04-01",
                    "httpMethod": "POST",
                    "name": "resourceGroups",
                    "requestHeaderDetails": {"commandName": "Microsoft.ResourceGraph"},
                    "content": {
                        "query": format!(
                            "ResourceContainers | where (name contains ('{}')) | where (type =~ ('Microsoft.Resources/subscriptions/resourcegroups')) | project id,name,type,kind,subscriptionId,resourceGroup | extend matchscore = name startswith '{}' | extend normalizedName = tolower(tostring(name)) | sort by matchscore desc, normalizedName asc | take 30",
                            q, q
                        ),
                    },
                },
                {
                    "url": "/providers/Microsoft.ResourceGraph/resources?api-version=2024-04-01",
                    "httpMethod": "POST",
                    "name": "resources",
                    "requestHeaderDetails": {"commandName": "Microsoft.ResourceGraph"},
                    "content": {
                        "query": format!("Resources | where name contains '{}' | take 30", q),
                    },
                },
            ],
        });

        let resp = self.client
            .post(endpoint)
            .header("User-Agent", "digse/0.0.1")
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: AzureBatchResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        let mut rank = 1usize;
        for item in &parsed.responses {
            if item.name == "resourceGroups" {
                for data in &item.content.data {
                    let url = format!(
                        "https://portal.azure.com/#@/resource/subscriptions/{}/resourceGroups/{}/overview",
                        data.subscription_id, data.name
                    );
                    let content = format!("Resource Group in Subscription: {}", data.subscription_id);
                    results.push(
                        SearchResult::new(data.name.clone(), url)
                            .with_snippet(content)
                            .with_engine(self.name())
                            .with_rank(query.offset + rank)
                            .with_score(1.0 - ((rank - 1) as f64 * 0.05))
                            .with_result_type(ResultType::IT),
                    );
                    rank += 1;
                }
            } else if item.name == "resources" {
                for data in &item.content.data {
                    let url = format!(
                        "https://portal.azure.com/#@/resource/subscriptions/{}/resourceGroups/{}/providers/{}/{}/overview",
                        data.subscription_id, data.resource_group, data.resource_type, data.name
                    );
                    let content = format!(
                        "Resource of type {} in Subscription: {}, Resource Group: {}",
                        data.resource_type, data.subscription_id, data.resource_group
                    );
                    results.push(
                        SearchResult::new(data.name.clone(), url)
                            .with_snippet(content)
                            .with_engine(self.name())
                            .with_rank(query.offset + rank)
                            .with_score(1.0 - ((rank - 1) as f64 * 0.05))
                            .with_result_type(ResultType::IT),
                    );
                    rank += 1;
                }
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for AzureEngine {
    fn name(&self) -> &str { &self.metadata.name }
    fn category(&self) -> EngineCategory { self.metadata.category }
    fn is_enabled(&self) -> bool { self.metadata.enabled }
    fn metadata(&self) -> EngineMetadata { self.metadata.clone() }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        self.fetch_results(query).await
    }

    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::IT | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://management.azure.com".to_string());
        s.insert("requires_auth".to_string(), "true".to_string());
        s
    }
}

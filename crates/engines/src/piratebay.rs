//! PirateBay search engine implementation
//!
//! Uses the public `apibay.org`
//! JSON endpoint (no API key), builds magnet links from `info_hash`, and
//! sorts results by seeders descending.

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Default trackers appended to every magnet link.
const TRACKERS: &[&str] = &[
    "udp://tracker.coppersurfer.tk:6969/announce",
    "udp://9.rarbg.to:2920/announce",
    "udp://tracker.opentrackr.org:1337",
    "udp://tracker.internetwarriors.net:1337/announce",
    "udp://tracker.leechers-paradise.org:6969/announce",
    "udp://tracker.pirateparty.gr:6969/announce",
    "udp://tracker.cyberia.is:6969/announce",
];

/// Pirate Bay torrent search engine
pub struct PirateBayEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Deserialize, Default)]
struct TpbTorrent {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    info_hash: String,
    #[serde(default)]
    seeders: String,
    #[serde(default)]
    leechers: String,
    #[serde(default)]
    size: String,
    #[serde(default)]
    added: String,
}

/// Humanize a byte count into e.g. "1.2 MiB".
fn humanize_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["Bytes", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes == 0 {
        return "0 Bytes".to_string();
    }
    let mut n = bytes as f64;
    let mut unit = 0usize;
    while n >= 1024.0 && unit < UNITS.len() - 1 {
        n /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.0} {}", n, UNITS[unit])
    }
}

impl PirateBayEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "piratebay".to_string(),
            category: EngineCategory::Files,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "The Pirate Bay - torrent search (apibay.org).".to_string(),
            website: Some("https://thepiratebay.org".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create PirateBay HTTP client");

        PirateBayEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let encoded = urlencoding::encode(&query.query);
        let url = format!("https://apibay.org/q.php?q={}&cat=0", encoded);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let torrents: Vec<TpbTorrent> = match response.json().await {
            Ok(v) => v,
            Err(_) => return Ok(vec![]),
        };

        // Empty result marker.
        if torrents
            .first()
            .map(|t| t.name == "No results returned")
            .unwrap_or(true)
        {
            return Ok(vec![]);
        }

        let mut rows: Vec<(usize, &TpbTorrent)> = torrents
            .iter()
            .map(|t| (t.seeders.parse::<usize>().unwrap_or(0), t))
            .collect();
        // Sort by seeders descending.
        rows.sort_by(|a, b| b.0.cmp(&a.0));

        let mut results = Vec::new();
        for (rank, (_, t)) in rows.iter().enumerate() {
            if results.len() >= query.count {
                break;
            }
            if t.name.is_empty() {
                continue;
            }
            let page_url = format!("https://thepiratebay.org/description.php?id={}", t.id);
            let tr: Vec<String> = TRACKERS.iter().map(|s| format!("&tr={}", s)).collect();
            let magnet = format!(
                "magnet:?xt=urn:btih:{}&dn={}{}",
                t.info_hash,
                urlencoding::encode(&t.name),
                tr.join("")
            );
            let filesize = t
                .size
                .parse::<u64>()
                .map(humanize_bytes)
                .unwrap_or_else(|_| t.size.clone());

            let mut result = SearchResult::new(t.name.clone(), page_url)
                .with_engine(self.name())
                .with_rank(query.offset + rank + 1)
                .with_score(1.0 - (rank as f64 * 0.05))
                .with_result_type(ResultType::Torrents)
                .with_extra("seeders", serde_json::json!(t.seeders))
                .with_extra("leechers", serde_json::json!(t.leechers))
                .with_extra("filesize", serde_json::json!(filesize))
                .with_extra("magnet", serde_json::json!(magnet))
                .with_extra("source", serde_json::json!("piratebay"));
            if let Ok(ts) = t.added.parse::<f64>() {
                result = result.with_extra("published_ts", serde_json::json!(ts));
            }
            results.push(result);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for PirateBayEngine {
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
        matches!(t, ResultType::Files | ResultType::Torrents | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("search_url".to_string(), "https://apibay.org/q.php".to_string());
        s
    }
}

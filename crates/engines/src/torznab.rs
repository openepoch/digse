//! Torznab search engine implementation
//!
//! Torznab is a standardized XML API
//! spoken by indexers such as Prowlarr and Jackett. The engine queries the
//! configured backend (`?t=search&q=…[&apikey=…][&cat=…]`) and parses the RSS
//! `<item>` elements, extracting title / size / seeders / leechers / magnet /
//! enclosure URL. Torznab attributes (`seeders`, `leechers`, `peers`,
//! `magneturl`) ride on `<torznab:attr name="…" value="…" />` child tags.
//!
//! Configuration is supplied via environment variables (mirrors the reference's
//! `settings.yml` fields): `TORZNAB_BASE_URL` (required), `TORZNAB_API_KEY`,
//! `TORZNAB_CATEGORIES` (comma-separated category IDs). When the base URL is
//! unset the engine degrades to returning no results.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Torznab (Jackett/Prowlarr) torrent/files search engine
pub struct TorznabEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    categories: Vec<String>,
}

impl TorznabEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "torznab".to_string(),
            category: EngineCategory::Files,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 20,
            description: "Torznab (Jackett/Prowlarr) indexer search.".to_string(),
            website: None,
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("Failed to create Torznab HTTP client");

        let categories = std::env::var("TORZNAB_CATEGORIES")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        TorznabEngine {
            metadata,
            client,
            base_url: std::env::var("TORZNAB_BASE_URL").unwrap_or_default(),
            api_key: std::env::var("TORZNAB_API_KEY").unwrap_or_default(),
            categories,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        // Graceful degradation when the backend is unconfigured (ref: init()).
        if self.base_url.is_empty() {
            tracing::info!("torznab requires TORZNAB_BASE_URL; returning empty");
            return Ok(vec![]);
        }

        let encoded = urlencoding::encode(&query.query);
        let mut search_url = format!(
            "{}?t=search&q={}",
            self.base_url.trim_end_matches('/'),
            encoded
        );
        if !self.api_key.is_empty() {
            search_url.push_str(&format!("&apikey={}", self.api_key));
        }
        if !self.categories.is_empty() {
            let cat = self.categories.join(",");
            search_url.push_str(&format!("&cat={}", cat));
        }

        let response = self
            .client
            .get(&search_url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/xml")
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

        Ok(parse_items(&text, query))
    }
}

/// Parse all `<item>` blocks from the RSS response into search results.
fn parse_items(xml: &str, query: &SearchQuery) -> Vec<SearchResult> {
    // Newznab error responses are `<error description="…" />`.
    if let Some(desc) = find_error(xml) {
        tracing::warn!("torznab API error: {}", desc);
        return vec![];
    }

    let mut results = Vec::new();
    for (i, block) in find_blocks(xml, "item").into_iter().enumerate() {
        if results.len() >= query.count {
            break;
        }
        let title = tag_text(block, "title").unwrap_or_default().to_string();
        let link = tag_text(block, "link").map(|s| s.to_string());
        let guid = tag_text(block, "guid").map(|s| s.to_string());
        let comments = tag_text(block, "comments").map(|s| s.to_string());
        let pub_date = tag_text(block, "pubDate").map(|s| s.to_string());
        let size = tag_text(block, "size").map(|s| s.to_string());
        let files = tag_text(block, "files").map(|s| s.to_string());

        let enclosure_url = enclosure_attr(block, "url");
        let enclosure_len = enclosure_attr(block, "length");

        let seeders = torznab_attr(block, "seeders");
        let leechers = torznab_attr(block, "leechers");
        let peers = torznab_attr(block, "peers");
        let magneturl = torznab_attr(block, "magneturl");

        let filesize_raw = if size.is_some() {
            size
        } else {
            enclosure_len.clone()
        };
        let filesize = filesize_raw
            .as_deref()
            .and_then(|s| s.parse::<u64>().ok())
            .map(humanize_bytes);

        let computed_leechers = map_leechers(&leechers, &seeders, &peers);

        // result url: guid(http) -> comments(http) -> enclosure(http).
        let result_url = map_first_http(&[guid.as_deref(), comments.as_deref()]);
        // magnet: magneturl -> guid(magnet) -> enclosure(magnet) -> link(magnet).
        let magnet = map_first_magnet(&[
            magneturl.as_deref(),
            guid.as_deref(),
            enclosure_url.as_deref(),
            link.as_deref(),
        ]);

        let url = result_url.clone().or_else(|| magnet.clone()).unwrap_or_default();
        if url.is_empty() {
            continue;
        }

        let mut result = SearchResult::new(title, url)
            .with_engine("torznab")
            .with_rank(query.offset + i + 1)
            .with_score(1.0 - (i as f64 * 0.05))
            .with_result_type(ResultType::Torrents)
            .with_extra("source", serde_json::json!("torznab"));
        if let Some(s) = &seeders {
            result = result.with_extra("seeders", serde_json::json!(s));
        }
        if let Some(l) = &computed_leechers {
            result = result.with_extra("leechers", serde_json::json!(l));
        }
        if let Some(f) = filesize {
            result = result.with_extra("filesize", serde_json::json!(f));
        }
        if let Some(m) = &magnet {
            result = result.with_extra("magnet", serde_json::json!(m));
        }
        if let Some(f) = files {
            result = result.with_extra("files", serde_json::json!(f));
        }
        if let Some(p) = pub_date {
            result = result.with_extra("published", serde_json::json!(p));
        }
        results.push(result);
    }
    results
}

// ---- minimal XML helpers (no XML crate dependency) -----------------------

/// Return `Some(desc)` if the document is a `<error description="…" />` node.
fn find_error(xml: &str) -> Option<String> {
    let s = xml.find("<error")?;
    let gt = xml[s..].find('>')?;
    let tag = &xml[s..s + gt];
    extract_attr(tag, "description")
}

/// Extract the inner text of each `<tag …>…</tag>` (non-nested) block.
fn find_blocks<'a>(text: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let close_len = close.len();
    let open_str = open.as_str();
    let close_str = close.as_str();
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(s) = rest.find(open_str) {
        let name_end = s + open.len();
        let nxt = rest[name_end..].chars().next();
        // The char right after the tag name must end the name: >, /, or whitespace.
        if !matches!(nxt, Some('>') | Some('/') | Some(' ') | Some('\t') | Some('\n') | Some('\r'))
        {
            rest = &rest[name_end..];
            continue;
        }
        let gt = match rest[s..].find('>') {
            Some(g) => s + g,
            None => break,
        };
        let body_start = gt + 1;
        let close_pos = match rest[body_start..].find(close_str) {
            Some(c) => body_start + c,
            None => break,
        };
        out.push(&rest[body_start..close_pos]);
        rest = &rest[close_pos + close_len..];
    }
    out
}

/// Text content of a single named child tag, e.g. `<title>…</title>`.
fn tag_text<'a>(block: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let s = block.find(open.as_str())?;
    let name_end = s + open.len();
    let nxt = block[name_end..].chars().next();
    if !matches!(nxt, Some('>') | Some('/') | Some(' ') | Some('\t') | Some('\n') | Some('\r')) {
        return None;
    }
    let gt = block[s..].find('>')?;
    let body_start = s + gt + 1;
    let close_pos = block[body_start..].find(close.as_str())?;
    Some(&block[body_start..body_start + close_pos])
}

/// `url`/`length` attribute of the `<enclosure … />` tag.
fn enclosure_attr(block: &str, attr: &str) -> Option<String> {
    let s = block.find("<enclosure")?;
    let gt = block[s..].find('>')?;
    extract_attr(&block[s..s + gt], attr)
}

/// `value` of `<torznab:attr name="{name}" value="…" />`.
fn torznab_attr(block: &str, name: &str) -> Option<String> {
    let mut rest = block;
    while let Some(s) = rest.find("<torznab:attr") {
        let after = &rest[s..];
        let gt = after.find('>')?;
        let tag = &after[..gt];
        rest = &after[gt..];
        match extract_attr(tag, "name") {
            Some(n) if n == name => return extract_attr(tag, "value"),
            _ => continue,
        }
    }
    None
}

/// Extract `attr="value"` from a tag string.
fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let key = format!("{}=\"", attr);
    let i = tag.find(key.as_str())?;
    let rest = &tag[i + key.len()..];
    let j = rest.find('"')?;
    Some(rest[..j].to_string())
}

/// Map leechers: explicit value, else peers - seeders.
fn map_leechers(leechers: &Option<String>, seeders: &Option<String>, peers: &Option<String>) -> Option<String> {
    if let Some(l) = leechers {
        return Some(l.clone());
    }
    if let (Some(s), Some(p)) = (seeders, peers) {
        if let (Ok(s), Ok(p)) = (s.parse::<i64>(), p.parse::<i64>()) {
            return Some((p - s).to_string());
        }
    }
    None
}

/// First value in the list starting with `http`.
fn map_first_http(values: &[Option<&str>]) -> Option<String> {
    values
        .iter()
        .flatten()
        .find(|v| v.starts_with("http"))
        .map(|s| s.to_string())
}

/// First value in the list starting with `magnet`.
fn map_first_magnet(values: &[Option<&str>]) -> Option<String> {
    values
        .iter()
        .flatten()
        .find(|v| v.starts_with("magnet"))
        .map(|s| s.to_string())
}

/// Approximate human-readable byte size.
fn humanize_bytes(n: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
    let mut size = n as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", n, UNITS[0])
    } else {
        format!("{:.2} {}", size, UNITS[unit])
    }
}

#[async_trait]
impl Engine for TorznabEngine {
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
        s.insert("base_url".to_string(), self.base_url.clone());
        s.insert(
            "show_magnet_links".to_string(),
            "true".to_string(),
        );
        s.insert(
            "show_torrent_files".to_string(),
            "false".to_string(),
        );
        s
    }
}

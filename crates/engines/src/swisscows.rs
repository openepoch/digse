//! Swisscows search engine implementation
//!
//! Queries the Swisscows `/v5/web/search` JSON API. The reference engine
//! generates `X-Request-Nonce`/`X-Request-Signature` headers using a Caesar
//! cipher, SHA-256 and base64url; these are implemented inline here (no extra
//! crates required). The web endpoint also returns a JWT-encoded `payload`
//! which is decoded inline.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
    TimeRange,
};

/// Swisscows general web search engine
pub struct SwisscowsEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://api.swisscows.com";
const RESULTS_PER_PAGE: usize = 20;

#[derive(Debug, Serialize, Deserialize)]
struct SwisscowsResponse {
    /// Web/category responses wrap the real items in a JWT `payload`.
    #[serde(default)]
    payload: Option<String>,
    /// Video responses return items directly.
    #[serde(default)]
    items: Vec<SwisscowsItem>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SwisscowsItem {
    #[serde(default)]
    #[serde(rename = "type")]
    item_type: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    thumbnail: SwisscowsThumbnail,
    #[serde(default)]
    #[serde(rename = "contentUrl")]
    content_url: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SwisscowsThumbnail {
    #[serde(default)]
    url: String,
}

// ----- signature helpers -----

const CAESAR_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const NONCE_ALPHABET: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

fn generate_nonce(len: usize) -> String {
    // Deterministic-ish PRNG seeded from a cheap entropy source (time).
    let mut state = simple_seed();
    let mut out = String::with_capacity(len);
    for _ in 0..len {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let idx = (state >> 33) as usize % NONCE_ALPHABET.len();
        out.push(NONCE_ALPHABET[idx] as char);
    }
    out
}

fn simple_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0xC0FFEE)
}

/// Caesar shift by `offset` that additionally inverts the casing of letters.
fn caesar_shift_with_switch_case(s: &str, offset: usize) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let upper = c.to_ascii_uppercase();
        if upper.is_ascii_uppercase() && (upper as u8) >= b'A' && (upper as u8) <= b'Z' {
            let idx = ((upper as u8) - b'A') as usize;
            let shifted = CAESAR_ALPHABET[(idx + offset) % CAESAR_ALPHABET.len()];
            let case_switched = if c.is_uppercase() {
                (shifted as char).to_ascii_lowercase()
            } else {
                shifted as char
            };
            out.push(case_switched);
        } else {
            out.push(c);
        }
    }
    out
}

fn sha256_b64url(s: &str) -> String {
    let digest = sha256(s.as_bytes());
    base64url_encode(&digest)
}

fn base64url_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        out.push(TABLE[(n & 0x3F) as usize] as char);
        i += 3;
    }
    let rem = bytes.len() - i;
    if rem == 1 {
        let n = (bytes[i] as u32) << 16;
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
    } else if rem == 2 {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
    }
    out
}

/// Minimal SHA-256 implementation (FIPS 180-4), self-contained.
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // Pad message.
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks_exact(4).enumerate().take(16) {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let mj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(mj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// Generate the (nonce, signature) pair used as Swisscows request headers.
fn generate_nonce_and_signature(base_path: &str, args: &[(String, String)]) -> (String, String) {
    let nonce = generate_nonce(32);
    let nonce_shifted = caesar_shift_with_switch_case(&nonce, 13);
    // keys sorted alphabetically, values plain (not URL-encoded).
    let mut sorted = args.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let query_string = sorted
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&");
    let full_path = format!("{}?{}", base_path, query_string);
    let signature = sha256_b64url(&format!("{}{}", full_path, nonce_shifted));
    (nonce, signature)
}

// ----- base64url decode (for the JWT payload) -----

fn base64url_decode(s: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut rev = [255u8; 256];
    for (i, b) in TABLE.iter().enumerate() {
        rev[*b as usize] = i as u8;
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let bytes = s.as_bytes();
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &b in bytes {
        let v = rev[b as usize];
        if v == 255 {
            continue;
        }
        buf = (buf << 6) | (v as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8 & 0xff);
        }
    }
    Some(out)
}

impl SwisscowsEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "swisscows".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Swisscows - privacy-friendly general web search.".to_string(),
            website: Some("https://swisscows.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Swisscows HTTP client");

        SwisscowsEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let pageno = (query.offset / RESULTS_PER_PAGE) + 1;
        let locale = "en-US".to_string();
        let freshness = match query.time_range {
            Some(TimeRange::Day) => "Day",
            Some(TimeRange::Week) => "Week",
            Some(TimeRange::Month) => "Month",
            Some(TimeRange::Year) => "Year",
            None => "All",
        }
        .to_string();
        let items_count = RESULTS_PER_PAGE.to_string();
        let offset = query.offset.to_string();

        let base_path = "/v5/web/search".to_string();
        let args = vec![
            ("freshness".to_string(), freshness.clone()),
            ("itemsCount".to_string(), items_count.clone()),
            ("locale".to_string(), locale.clone()),
            ("offset".to_string(), offset.clone()),
            ("query".to_string(), query.query.clone()),
            ("spellcheck".to_string(), "true".to_string()),
        ];

        let (nonce, signature) = generate_nonce_and_signature(&base_path, &args);

        // Build the query string (URL-encoded) for the actual request.
        let url = format!("{}{}", BASE_URL, base_path);
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("X-Request-Nonce", &nonce)
            .header("X-Request-Signature", &signature)
            .query(&[
                ("freshness", freshness.as_str()),
                ("itemsCount", items_count.as_str()),
                ("locale", locale.as_str()),
                ("offset", offset.as_str()),
                ("query", query.query.as_str()),
                ("spellcheck", "true"),
            ])
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

        // The web payload is JWT-encoded (payload.split('.')[1]); decode it.
        let parsed: SwisscowsResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let items = if let Some(payload) = &parsed.payload {
            match decode_jwt_payload(payload) {
                Some(decoded) => decoded.items,
                None => Vec::new(),
            }
        } else {
            parsed.items
        };

        let mut results = Vec::new();
        for (i, item) in items.iter().enumerate() {
            if item.item_type != "WebPage" {
                continue;
            }
            let url = if !item.url.is_empty() {
                item.url.clone()
            } else {
                continue;
            };
            let title = if !item.name.is_empty() {
                item.name.clone()
            } else {
                item.title.clone()
            };
            let thumbnail = item.thumbnail.url.clone();

            let mut result = SearchResult::new(title, url)
                .with_snippet(item.description.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web);

            if !thumbnail.is_empty() {
                result = result.with_extra("thumbnail", serde_json::json!(thumbnail));
            }

            results.push(result);
        }

        Ok(results)
    }
}

fn decode_jwt_payload(payload: &str) -> Option<SwisscowsResponse> {
    let parts: Vec<&str> = payload.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let segment = parts[1];
    let bytes = base64url_decode(segment)?;
    let json_str = String::from_utf8(bytes).ok()?;
    serde_json::from_str(&json_str).ok()
}

#[async_trait]
impl Engine for SwisscowsEngine {
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
        s.insert("base_url".into(), BASE_URL.into());
        s.insert("swisscows_category".into(), "web".into());
        s.insert("results_per_page".into(), RESULTS_PER_PAGE.to_string());
        s
    }
}

//! Seek.ninja search engine implementation
//!
//! The reference solves
//! a SHA256 Proof-of-Work challenge before issuing the real search request;
//! digse implements the PoW solver (with a self-contained SHA256 — no external
//! crypto crate is available), but if the challenge cannot be fetched or solved
//! the engine returns a graceful empty result list.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Seek.ninja search engine (general/web, PoW-protected)
pub struct SeekNinjaEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl SeekNinjaEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "seekninja".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 20,
            description: "Seek.ninja - privacy-oriented web search (PoW-protected).".to_string(),
            website: Some("https://seek.ninja".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("Failed to create seek.ninja HTTP client");
        SeekNinjaEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://seek.ninja";
        let challenge = match self.get_challenge(&query.query).await {
            Some(c) => c,
            None => return Ok(vec![]),
        };
        let solutions = solve_pow(&challenge);
        let panswers = solutions
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let url = format!("{}/search-sse", base_url);
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "text/event-stream")
            .query(&[
                ("q", query.query.as_str()),
                ("panswers", panswers.as_str()),
                ("pid", challenge.challenge_id.as_str()),
                ("adult", "moderate"),
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
        Ok(self.parse_sse(&text))
    }

    /// Fetch the search page and extract the embedded `pow: {...}` challenge.
    async fn get_challenge(&self, query: &str) -> Option<PowChallenge> {
        let encoded = urlencoding::encode(query);
        let url = format!("https://seek.ninja/s?q={}", encoded);
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let text = resp.text().await.ok()?;
        let start = text.find("pow: {")?;
        let after = &text[start + "pow: {".len()..];
        let end = after.find("},")?;
        let raw = format!("{{{}}}", &after[..end]);
        serde_json::from_str(&raw).ok()
    }

    fn parse_sse(&self, text: &str) -> Vec<SearchResult> {
        let mut results = Vec::new();
        // The response is a stream of server-side events separated by blank lines.
        for event in text.split("\n\n") {
            let mut lines = event.lines();
            let event_name = match lines.next() {
                Some(l) => l,
                None => continue,
            };
            let data = match lines.next() {
                Some(l) => l,
                None => continue,
            };
            if !event_name.ends_with("resultsUpdate") {
                continue;
            }
            let payload = match data.strip_prefix("data: ") {
                Some(p) => p,
                None => continue,
            };
            #[derive(Deserialize)]
            struct Update {
                #[serde(default)]
                results: Vec<SeekResult>,
            }
            let parsed: Update = match serde_json::from_str(payload) {
                Ok(p) => p,
                Err(_) => continue,
            };
            for r in parsed.results {
                if r.url.is_empty() {
                    continue;
                }
                results.push(
                    SearchResult::new(strip_html(&r.title), r.url)
                        .with_snippet(strip_html(&r.blurb))
                        .with_engine(self.name())
                        .with_result_type(ResultType::Web),
                );
            }
        }
        results
    }
}

fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.trim().to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PowChallenge {
    #[serde(default)]
    nonce: String,
    #[serde(default)]
    k: serde_json::Value,
    #[serde(default)]
    indifficulty: f64,
    #[serde(rename = "challengeId")]
    #[serde(default)]
    challenge_id: String,
}

/// Solve a SHA256 PoW challenge: find `k` integers `ans` such that
/// `sha256(nonce || ans)` begins with `leading` zero hex digits.
fn solve_pow(challenge: &PowChallenge) -> Vec<i64> {
    let k = challenge
        .k
        .as_i64()
        .or_else(|| challenge.k.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(1)
        .max(1) as usize;
    let indifficulty = challenge.indifficulty;
    let leading = indifficulty as usize;
    let frac = indifficulty - (leading as f64);
    let prefix: String = "0".repeat(leading);
    let max_nib = if frac > 0.0 {
        15 - (frac * 16.0) as i64
    } else {
        15
    };
    let mut nonce_input = Vec::with_capacity(challenge.nonce.len() + 24);
    nonce_input.extend_from_slice(challenge.nonce.as_bytes());
    let nonce_len = nonce_input.len();
    let mut solutions = Vec::new();
    let mut ans: i64 = 0;
    while solutions.len() < k {
        let ans_str = ans.to_string();
        nonce_input.truncate(nonce_len);
        nonce_input.extend_from_slice(ans_str.as_bytes());
        let digest = sha256(&nonce_input);
        if digest.starts_with(prefix.as_str())
            && (frac <= 0.0
                || digest
                    .as_bytes()
                    .get(leading)
                    .and_then(|c| (*c as char).to_digit(16))
                    .map(|d| d as i64 <= max_nib)
                    .unwrap_or(true))
        {
            solutions.push(ans);
        }
        ans += 1;
        // safety valve to avoid infinite loops on malformed challenges
        if ans > 5_000_000 {
            break;
        }
    }
    solutions
}

/// Compute the lowercase hex SHA256 digest of `data`.
fn sha256(data: &[u8]) -> String {
    // SHA-256 constants
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

    // pre-processing: padding
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    // process each 512-bit chunk
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
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
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
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

    let mut out = String::with_capacity(64);
    for word in h.iter() {
        out.push_str(&format!("{:08x}", word));
    }
    out
}

#[derive(Debug, Deserialize)]
struct SeekResult {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    blurb: String,
}

#[async_trait]
impl Engine for SeekNinjaEngine {
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
        let mut results = self.fetch_results(query).await?;
        for (i, r) in results.iter_mut().enumerate() {
            r.rank = query.offset + i + 1;
            r.score = 1.0 - (i as f64 * 0.05);
        }
        Ok(results)
    }
    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Web | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://seek.ninja".into());
        s
    }
}

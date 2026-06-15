//! Tiger search engine implementation
//!
//! The reference engine solves a math CAPTCHA to obtain a session cookie
//! before querying `https://tiger.ch/Websuche`. This implementation attempts
//! the same flow and degrades gracefully (empty results) on any failure.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Tiger (Swiss meta) general web search engine
pub struct TigerEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://tiger.ch";

impl TigerEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "tiger".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Tiger - Swiss meta search engine.".to_string(),
            website: Some("https://tiger.ch".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Tiger HTTP client");

        TigerEngine { metadata, client }
    }

    /// Solve Tiger's math CAPTCHA and return a session code (the value of the
    /// `Code=` portion of the `Tiger.ch` cookie). Returns `None` on any failure.
    async fn obtain_session_code(&self) -> Option<String> {
        // 1. GET the intern code page (carries ASP.NET hidden fields + cookies).
        let entry = self
            .client
            .get(format!("{}/_internCode.aspx", BASE_URL))
            .header("User-Agent", "digse/0.0.1")
            .send()
            .await
            .ok()?;
        if !entry.status().is_success() {
            return None;
        }
        let entry_html = entry.text().await.ok()?;
        let viewstate = extract_input_value(&entry_html, "__VIEWSTATE")?;
        let viewgen = extract_input_value(&entry_html, "__VIEWSTATEGENERATOR")
            .unwrap_or_default();
        let eventval = extract_input_value(&entry_html, "__EVENTVALIDATION")
            .unwrap_or_default();

        // 2. Generate three random numbers and ask the server for the operands.
        let mut rng_state = simple_seed();
        let num1 = next_rand(&mut rng_state, 11, 19);
        let num2 = next_rand(&mut rng_state, 1, 9);
        let num3 = next_rand(&mut rng_state, 1, 9);

        let challenge = self
            .client
            .get(format!(
                "{}/Services/Human.svc/Make?M1={}&M2={}&M3={}",
                BASE_URL, num1, num2, num3
            ))
            .header("User-Agent", "digse/0.0.1")
            .send()
            .await
            .ok()?;
        if !challenge.status().is_success() {
            return None;
        }
        let challenge_text = challenge.text().await.ok()?;
        // The response JSON has shape {"d": "[[{\"Z1\":\"+\",\"Z2\":\"-\"}]]"}.
        let z1 = extract_json_field(&challenge_text, "Z1").unwrap_or_else(|| "+".to_string());
        let z2 = extract_json_field(&challenge_text, "Z2").unwrap_or_else(|| "+".to_string());

        let mut result = num1 as i64;
        result = apply_op(result, num2 as i64, &z1);
        result = apply_op(result, num3 as i64, &z2);

        // 3. POST the answer.
        let txt_m = result.to_string();
        let form = vec![
            ("__VIEWSTATE", viewstate.as_str()),
            ("__VIEWSTATEGENERATOR", viewgen.as_str()),
            ("__EVENTVALIDATION", eventval.as_str()),
            ("txtM", txt_m.as_str()),
            ("btnHuman", "OK"),
        ];
        let post_resp = self
            .client
            .post(format!("{}/_internCode.aspx", BASE_URL))
            .header("User-Agent", "digse/0.0.1")
            .form(&form)
            .send()
            .await
            .ok()?;
        if !post_resp.status().is_success() {
            return None;
        }

        // Read the Tiger.ch cookie from the response cookie jar.
        for cookie in post_resp.cookies() {
            if cookie.name() == "Tiger.ch" {
                let val = cookie.value();
                if let Some(code) = extract_between(val, "Code=", "&") {
                    if !code.is_empty() {
                        return Some(code.to_string());
                    }
                }
                // value may end with the code (no trailing &)
                if let Some(idx) = val.find("Code=") {
                    let code = &val[idx + "Code=".len()..];
                    if !code.is_empty() {
                        return Some(code.to_string());
                    }
                }
            }
        }
        None
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let code = match self.obtain_session_code().await {
            Some(c) => c,
            None => {
                tracing::info!("tiger: could not obtain session code; returning empty");
                return Ok(vec![]);
            }
        };

        let pageno = (query.offset / 10) + 1;
        let page = pageno.to_string();
        let cookie_value = format!("Code={}", code);

        let resp = self
            .client
            .get(format!("{}/Websuche", BASE_URL))
            .header("User-Agent", "digse/0.0.1")
            .header("Cookie", format!("Tiger.ch={}", cookie_value))
            .query(&[("w", query.query.as_str()), ("page", page.as_str())])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let html = resp
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        Ok(self.parse_html(html, query))
    }

    fn parse_html(&self, html: String, query: &SearchQuery) -> Vec<SearchResult> {
        use scraper::{Html, Selector};

        let document = Html::parse_document(&html);
        let mut results = Vec::new();

        let row_sel = match Selector::parse("div#mainContainer table tr") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let weblink_sel = Selector::parse("a.weblink").unwrap();
        let body_sel = Selector::parse("*[class*='webbodynopic']").unwrap();

        for (i, row) in document.select(&row_sel).enumerate() {
            let a = match row.select(&weblink_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let url = a.value().attr("href").unwrap_or("").to_string();
            if url.is_empty() {
                continue;
            }
            let title = a.text().collect::<String>().trim().to_string();
            let content = row
                .select(&body_sel)
                .next()
                .map(|c| c.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            if title.is_empty() && url.is_empty() {
                continue;
            }
            results.push(
                SearchResult::new(title, url)
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Web),
            );
        }

        results
    }
}

// ---------- small helpers ----------

fn simple_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0xC0FFEE)
}

fn next_rand(state: &mut u64, lo: i32, hi: i32) -> i32 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let span = (hi - lo + 1) as u64;
    lo + ((*state >> 33) % span) as i32
}

fn apply_op(a: i64, b: i64, op: &str) -> i64 {
    match op {
        "+" => a + b,
        "-" => a - b,
        _ => a,
    }
}

/// Read the value of a hidden `<input name="X" value="...">` element.
fn extract_input_value(html: &str, name: &str) -> Option<String> {
    let needle = format!("name=\"{}\"", name);
    let start = html.find(&needle)?;
    let after = &html[start..];
    let vrel = after.find("value=\"")?;
    let vstart = vrel + "value=\"".len();
    let slice = &after[vstart..];
    let end = slice.find('"')?;
    Some(slice[..end].to_string())
}

/// Extract a string field from a JSON-ish blob (e.g. "Z1":"+").
fn extract_json_field(text: &str, field: &str) -> Option<String> {
    let needle = format!("\"{}\":\"", field);
    let start = text.find(&needle)?;
    let vstart = start + needle.len();
    let slice = &text[vstart..];
    let end = slice.find('"')?;
    Some(slice[..end].to_string())
}

/// Extract the substring between `start_marker` and `end_marker`, first match.
fn extract_between<'a>(s: &'a str, start_marker: &str, end_marker: &str) -> Option<&'a str> {
    let srel = s.find(start_marker)?;
    let vstart = srel + start_marker.len();
    let rest = &s[vstart..];
    let end = rest.find(end_marker)?;
    Some(&rest[..end])
}

#[async_trait]
impl Engine for TigerEngine {
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
        s.insert("tiger_category".into(), "Websuche".into());
        s.insert("results".into(), "HTML".into());
        s
    }
}

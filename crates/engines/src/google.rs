//! Google web search engine implementation
//!
//! HTML scrape of Google's results page.
//! The reference iterates over `//a[@data-ved and not(@class)]`, reads the
//! title from the first `.//div[@style]` child, decodes the `/url?q=` redirect,
//! and pulls a snippet from `../../div[contains(@class, "ilUpNd H66NU aSRlid")]`.
//! Category: general/web. Graceful empty on captcha/sorry pages.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Google general web search engine (HTML scrape)
pub struct GoogleEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl GoogleEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "google".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Google - general web search.".to_string(),
            website: Some("https://www.google.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Google HTTP client");

        GoogleEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        // ref: start = (pageno - 1) * 10
        let start = query.offset * 10;
        let start_str = start.to_string();
        let url = "https://www.google.com/search";

        let response = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "*/*")
            .header("Accept-Language", "en-US,en;q=0.9")
            .query(&[
                ("q", query.query.as_str()),
                ("hl", "en-US"),
                ("lr", "lang_en"),
                ("ie", "utf8"),
                ("oe", "utf8"),
                ("filter", "0"),
                ("start", start_str.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let html = response
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        // ref detect_google_sorry: short body containing /sorry/ -> captcha
        if html.len() < 2000 && html.contains("/sorry/") {
            return Ok(vec![]);
        }

        let doc = Html::parse_document(&html);

        // ref: //a[@data-ved and not(@class)]
        // scraper has no `not()`/attribute-presence combo that maps cleanly, so
        // we select all `a[data-ved]` and filter out those carrying a class.
        let candidate_sel = match Selector::parse("a[data-ved]") {
            Ok(s) => s,
            Err(_) => return Ok(vec![]),
        };
        let style_div_sel = Selector::parse("div[style]").unwrap();
        let content_sel =
            Selector::parse("div.ilUpNd.H66NU.aSRlid").or_else(|_| Selector::parse("div")).unwrap();
        let img_sel = Selector::parse("img").unwrap();

        let mut results = Vec::new();
        let mut i = 0usize;
        for el in doc.select(&candidate_sel) {
            if i >= query.count {
                break;
            }
            // skip anchors that carry a class attribute (ref: not(@class))
            if el.value().attr("class").map(|c| !c.is_empty()).unwrap_or(false) {
                continue;
            }
            // title: first descendant div[@style]
            let title_el = match el.select(&style_div_sel).next() {
                Some(t) => t,
                None => continue,
            };
            let title = title_el.text().collect::<String>().trim().to_string();
            if title.is_empty() {
                continue;
            }
            let raw_url = match el.value().attr("href") {
                Some(h) => h.to_string(),
                None => continue,
            };
            let url_decoded = decode_google_redirect(&raw_url);
            if url_decoded.is_empty() {
                continue;
            }
            // content: sibling/ancestor descendant matching the content class
            let content = pick_content(&el, &content_sel);

            // thumbnail: first descendant img (ref keeps non-favicon images)
            let thumbnail = el
                .select(&img_sel)
                .next()
                .and_then(|img| img.value().attr("src"))
                .map(|s| {
                    if s.starts_with("data:image") {
                        String::new()
                    } else {
                        s.to_string()
                    }
                })
                .unwrap_or_default();

            let mut result = SearchResult::new(title, url_decoded)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web)
                .with_extra("source", serde_json::json!("google"));
            if !thumbnail.is_empty() {
                result = result.with_extra("thumbnail", serde_json::json!(thumbnail));
            }
            results.push(result);
            i += 1;
        }

        Ok(results)
    }
}

// ref: if raw_url starts with "/url?q=" strip it and the trailing "&sa=U",
// percent-decoding the remainder. Otherwise return the raw url.
fn decode_google_redirect(raw: &str) -> String {
    if let Some(rest) = raw.strip_prefix("/url?q=") {
        let end = rest.find("&sa=U").unwrap_or(rest.len());
        let encoded = &rest[..end];
        percent_decode(encoded)
    } else {
        raw.to_string()
    }
}

// Minimal percent-decoder sufficient for Google redirect URLs (decodes %XX and
// '+' -> ' '). Avoids needing an extra crate dependency.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(((hi << 4) | lo) as u8);
                i += 3;
                continue;
            }
        }
        if b == b'+' {
            out.push(b' ');
        } else {
            out.push(b);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

// ref: content_nodes = eval_xpath(result, '../..//div[contains(@class, "...")]')
// `ElementRef` in scraper 0.18 does not expose ancestor walking, so we fall
// back to searching the anchor's own descendants. In Google's markup the
// snippet usually lives in a sibling of the title inside the same container;
// when the specific content class isn't found we return the longest descendant
// div text as a reasonable approximation.
fn pick_content(el: &scraper::ElementRef, sel: &Selector) -> String {
    if let Some(specific) = Selector::parse("div.ilUpNd, div.H66NU, div.aSRlid").ok() {
        if let Some(found) = el.select(&specific).next() {
            let text = found.text().collect::<String>().trim().to_string();
            if !text.is_empty() {
                return text;
            }
        }
    }
    // Fallback: pick the longest descendant div text as the snippet.
    let mut best = String::new();
    for d in el.select(sel) {
        let text = d.text().collect::<String>().trim().to_string();
        if text.len() > best.len() {
            best = text;
        }
    }
    best
}

#[async_trait]
impl Engine for GoogleEngine {
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
        settings.insert("base_url".to_string(), "https://www.google.com".to_string());
        settings.insert("search_endpoint".to_string(), "/search".to_string());
        settings.insert("subdomain".to_string(), "www.google.com".to_string());
        settings
    }
}

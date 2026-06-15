//! Yandex search engine implementation
//!
//! Two paths, selected by the query's
//! `result_type`:
//!
//! * **web** — HTML scrape of `yandex.com/search/site/`. Results live in
//!   `li.serp-item`; title/url/content extracted by class selectors. A
//!   `x-yandex-captcha: captcha` response header means Yandex is blocking us;
//!   the engine returns no results.
//! * **images** — the markup embeds a JSON blob between fixed markers; we
//!   extract it with `extr`, parse it, and walk
//!   `initialState.serpList.items.entities`. For each image we pick the
//!   largest duplicate (max height) as `img_src`. This path is best-effort:
//!   if the markers are absent (markup changed) it gracefully returns `[]`.

use async_trait::async_trait;
use scraper::{Html, Selector};
use serde_json::Value;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

const BASE_URL_WEB: &str = "https://yandex.com/search/site/";
const BASE_URL_IMAGES: &str = "https://yandex.com/images/search";
const YANDEX_COOKIE: &str =
    "yp=1716337604.sp.family%3A0#1685406411.szm.1:1920x1080:1920x999";

/// Yandex general (web + images) search engine
pub struct YandexEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl YandexEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "yandex".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Yandex web/image search.".to_string(),
            website: Some("https://yandex.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Yandex HTTP client");

        YandexEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let want_images = matches!(query.result_type, ResultType::Images);
        let pageno = (query.offset / query.count.max(1)) + 1;
        let encoded = urlencoding::encode(&query.query);

        let (url, is_images) = if want_images {
            let mut u = format!("{}?text={}&uinfo=sw-1920-sh-1080-ww-1125-wh-999", BASE_URL_IMAGES, encoded);
            if pageno > 1 {
                u.push_str(&format!("&p={}", pageno - 1));
            }
            (u, true)
        } else {
            let mut u = format!(
                "{}?tmpl_version=releases&text={}&web=1&frame=1&searchid=3131712",
                BASE_URL_WEB, encoded
            );
            if let Some(lang) = query.language.as_deref() {
                let l = lang.split('-').next().unwrap_or("");
                if YANDEX_SUPPORTED_LANGS.contains(&l) {
                    u.push_str(&format!("&lang={}", l));
                }
            }
            if pageno > 1 {
                u.push_str(&format!("&p={}", pageno - 1));
            }
            (u, false)
        };

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (digse/0.1.0)")
            .header("Accept", "text/html,application/xhtml+xml")
            .header("Cookie", YANDEX_COOKIE)
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        // Captcha wall -> no results (ref: catch_bad_response).
        if response
            .headers()
            .get("x-yandex-captcha")
            .map(|v| v == "captcha")
            .unwrap_or(false)
        {
            tracing::info!("yandex returned a captcha challenge; returning empty");
            return Ok(vec![]);
        }

        if !response.status().is_success() {
            return Ok(vec![]);
        }
        let text = match response.text().await {
            Ok(t) => t,
            Err(_) => return Ok(vec![]),
        };

        if is_images {
            Ok(parse_images(&text, query))
        } else {
            parse_web(&text, query)
        }
    }
}

/// Web results: `li.serp-item` rows.
fn parse_web(text: &str, query: &SearchQuery) -> Result<Vec<SearchResult>> {
    let doc = Html::parse_document(text);
    let item_sel = match Selector::parse("li.serp-item") {
        Ok(s) => s,
        Err(_) => return Ok(vec![]),
    };
    let link_sel = Selector::parse("a.b-serp-item__title-link").unwrap();
    let title_sel = Selector::parse("h3.b-serp-item__title a.b-serp-item__title-link span").unwrap();
    let content_sel = Selector::parse("div.b-serp-item__content div.b-serp-item__text").unwrap();

    let mut results = Vec::new();
    for (i, item) in doc.select(&item_sel).enumerate() {
        if results.len() >= query.count {
            break;
        }
        let link = match item.select(&link_sel).next() {
            Some(l) => l,
            None => continue,
        };
        let url = link.value().attr("href").unwrap_or("").to_string();
        if url.is_empty() {
            continue;
        }
        let title = text_of(item.select(&title_sel).next());
        if title.is_empty() {
            continue;
        }
        let content = text_of(item.select(&content_sel).next());

        let result = SearchResult::new(title, url)
            .with_snippet(content)
            .with_engine("yandex")
            .with_rank(query.offset + i + 1)
            .with_score(1.0 - (i as f64 * 0.05))
            .with_result_type(ResultType::Web)
            .with_extra("source", serde_json::json!("yandex"));
        results.push(result);
    }
    Ok(results)
}

/// Image results: extract the embedded JSON blob and walk its entities.
fn parse_images(text: &str, query: &SearchQuery) -> Vec<SearchResult> {
    // Unescape HTML entities so the JSON markers line up with the source.
    let unescaped = text
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");

    let start_marker = "{\"location\":\"/images/search/";
    let (content, ending) = match extr(&unescaped, start_marker, "advRsyaSearchColumn\":null}}") {
        Some(c) => (c, "advRsyaSearchColumn\":null}}"),
        None => match extr(&unescaped, start_marker, "false}}}") {
            Some(c) => (c, "false}}}"),
            None => return vec![],
        },
    };
    let json_str = format!("{}{}{}", start_marker, content, ending);

    let json: Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    let entities = json
        .get("initialState")
        .and_then(|v| v.get("serpList"))
        .and_then(|v| v.get("items"))
        .and_then(|v| v.get("entities"))
        .and_then(|v| v.as_object());

    let entities = match entities {
        Some(e) => e,
        None => return vec![],
    };

    let mut results = Vec::new();
    for (i, item_data) in entities.values().enumerate() {
        if results.len() >= query.count {
            break;
        }
        let title = item_data
            .get("snippet")
            .and_then(|v| v.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let source = item_data
            .get("snippet")
            .and_then(|v| v.get("url"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if source.is_empty() {
            continue;
        }

        // Pick the highest-resolution duplicate as image_source.
        let viewer = item_data.get("viewerData");
        let mut image_source = viewer
            .and_then(|v| v.get("thumb"))
            .cloned()
            .unwrap_or(Value::Null);
        let mut best_h = image_source.get("h").and_then(|v| v.as_i64()).unwrap_or(0);
        for arr_key in ["dups", "preview"] {
            if let Some(arr) = viewer.and_then(|v| v.get(arr_key)).and_then(|v| v.as_array()) {
                for cand in arr {
                    let h = cand.get("h").and_then(|v| v.as_i64()).unwrap_or(0);
                    if h > best_h {
                        best_h = h;
                        image_source = cand.clone();
                    }
                }
            }
        }

        let img_src = image_source
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let w = image_source.get("w").and_then(|v| v.as_i64()).unwrap_or(0);
        let h = image_source.get("h").and_then(|v| v.as_i64()).unwrap_or(0);
        let filesize = image_source
            .get("fileSizeInBytes")
            .and_then(|v| v.as_i64())
            .map(humanize_bytes)
            .unwrap_or_default();
        let thumbnail = item_data
            .get("image")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let resolution = format!("{} x {}", w, h);

        let result = SearchResult::new(title, source)
            .with_engine("yandex")
            .with_rank(query.offset + i + 1)
            .with_score(1.0 - (i as f64 * 0.05))
            .with_result_type(ResultType::Images)
            .with_extra("img_src", serde_json::json!(img_src))
            .with_extra("thumbnail", serde_json::json!(thumbnail))
            .with_extra("resolution", serde_json::json!(resolution))
            .with_extra("filesize", serde_json::json!(filesize))
            .with_extra("source", serde_json::json!("yandex"));
        results.push(result);
    }
    results
}

/// Extract the substring between `start` and `end` markers. Returns `None` if
/// either marker is absent.
fn extr<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let s = text.find(start)? + start.len();
    let rest = &text[s..];
    let e = rest.find(end)?;
    Some(&rest[..e])
}

/// Approximate human-readable byte size (e.g. `1.2 MB`).
fn humanize_bytes(n: i64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
    if n < 0 {
        return n.to_string();
    }
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

/// Concatenate the direct text content of an optional element.
fn text_of(el: Option<scraper::ElementRef>) -> String {
    match el {
        Some(e) => e.text().collect::<String>().trim().to_string(),
        None => String::new(),
    }
}

/// Languages Yandex accepts in the `lang` param (ref: `yandex_supported_langs`).
const YANDEX_SUPPORTED_LANGS: &[&str] = &[
    "ru", "en", "be", "fr", "de", "id", "kk", "tt", "tr", "uk",
];

#[async_trait]
impl Engine for YandexEngine {
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
        matches!(t, ResultType::Web | ResultType::Images | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url_web".to_string(), BASE_URL_WEB.to_string());
        s.insert("base_url_images".to_string(), BASE_URL_IMAGES.to_string());
        s.insert("paging".to_string(), "true".to_string());
        s
    }
}

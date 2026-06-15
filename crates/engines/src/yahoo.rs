//! Yahoo search engine implementation
//!
//! HTML scrape of the Yahoo results
//! page (`div.algo-sr`). Builds the `sB` cookie (safesearch + language) and
//! strips Yahoo's tracking wrapper (`/RU=.../RK` / `/RS`) from result URLs via
//! `parse_url`. Region/domain mapping mirrors the reference (defaults to
//! `search.yahoo.com`).

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
    TimeRange,
};

/// Yahoo general web search engine
pub struct YahooEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl YahooEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "yahoo".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Yahoo web search.".to_string(),
            website: Some("https://search.yahoo.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Yahoo HTTP client");

        YahooEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let domain = "search.yahoo.com";
        let pageno = (query.offset / query.count.max(1)) + 1;
        let lang = query
            .language
            .as_deref()
            .map(|l| l.split('-').next().unwrap_or("en"))
            .unwrap_or("en");

        // URL params (ref: p, btf, iscqry/b/pz/bct/xargs).
        let p = urlencoding::encode(&query.query);
        let mut params: Vec<String> = vec![format!("p={}", p)];
        match query.time_range {
            Some(TimeRange::Day) => params.push("btf=d".to_string()),
            Some(TimeRange::Week) => params.push("btf=w".to_string()),
            Some(TimeRange::Month) => params.push("btf=m".to_string()),
            _ => {}
        }
        if pageno == 1 {
            params.push("iscqry=".to_string());
        } else {
            let b = pageno * 7 + 1;
            params.push(format!("b={}", b));
            params.push("pz=7".to_string());
            params.push("bct=0".to_string());
            params.push("xargs=0".to_string());
        }
        let url = format!("https://{}/search?{}", domain, params.join("&"));

        // sB cookie: v / vm(safesearch) / fl / vl(lang) / pn / rw / userset.
        let vm = if query.safe_search { "r" } else { "p" };
        let sb = format!(
            "v=1&vm={}&fl=1&vl=lang_{}&pn=10&rw=new&userset=1",
            vm, lang
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (digse/0.1.0)")
            .header("Accept", "text/html,application/xhtml+xml")
            .header("Accept-Language", lang)
            .header("Cookie", format!("sB={}", sb))
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }
        let text = match response.text().await {
            Ok(t) => t,
            Err(_) => return Ok(vec![]),
        };

        self.parse(&text, query)
    }

    fn parse(&self, html: &str, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let doc = Html::parse_document(html);
        let algo_sel = match Selector::parse("div.algo-sr") {
            Ok(s) => s,
            Err(_) => return Ok(vec![]),
        };
        let comp_sel = Selector::parse("div.compTitle").unwrap();
        let a_sel = Selector::parse("a").unwrap();
        let span_sel = Selector::parse("h3 span").unwrap();
        let content_sel = Selector::parse("div.compText").unwrap();

        let mut results = Vec::new();
        for (i, algo) in doc.select(&algo_sel).enumerate() {
            if results.len() >= query.count {
                break;
            }
            let comp = match algo.select(&comp_sel).next() {
                Some(c) => c,
                None => continue,
            };
            let a = match comp.select(&a_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let raw_href = a.value().attr("href").unwrap_or("");
            if raw_href.is_empty() {
                continue;
            }
            let url = parse_url(raw_href);
            if url.is_empty() {
                continue;
            }
            // Title: span text, else a[aria-label], else anchor text.
            let mut title = text_of(comp.select(&span_sel).next());
            if title.is_empty() {
                title = a
                    .value()
                    .attr("aria-label")
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
            }
            if title.is_empty() {
                title = a.text().collect::<String>().trim().to_string();
            }
            if title.is_empty() {
                continue;
            }
            let content = text_of(algo.select(&content_sel).next());

            let result = SearchResult::new(title, url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web)
                .with_extra("source", serde_json::json!("yahoo"));
            results.push(result);
        }
        Ok(results)
    }
}

/// Strip Yahoo's tracking wrapper from a result URL.
///
/// Port of `parse_url` in `yahoo.py`: locate the real URL after `/RU=`, then cut
/// before the trailing `/RS` or `/RK` marker, finally percent-decoding.
fn parse_url(url_string: &str) -> String {
    let ru = match url_string.find("/RU=") {
        Some(i) => i,
        None => return url_string.to_string(),
    };
    let after_ru = &url_string[ru + 4..];
    let start = match after_ru.find("http") {
        Some(j) => ru + 4 + j,
        None => return url_string.to_string(),
    };
    let mut endpositions = Vec::new();
    for ending in ["/RS", "/RK"] {
        if let Some(endpos) = url_string.rfind(ending) {
            endpositions.push(endpos);
        }
    }
    if start == 0 || endpositions.is_empty() {
        return url_string.to_string();
    }
    let end = *endpositions.iter().min().unwrap();
    if end <= start {
        return url_string.to_string();
    }
    let raw = &url_string[start..end];
    urlencoding::decode(raw)
        .map(|c| c.into_owned())
        .unwrap_or_else(|_| raw.to_string())
}

/// Concatenate the direct text content of an optional element.
fn text_of(el: Option<scraper::ElementRef>) -> String {
    match el {
        Some(e) => e.text().collect::<String>().trim().to_string(),
        None => String::new(),
    }
}

#[async_trait]
impl Engine for YahooEngine {
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
            "base_url".to_string(),
            "https://search.yahoo.com/search".to_string(),
        );
        s.insert("paging".to_string(), "true".to_string());
        s.insert("time_range_support".to_string(), "true".to_string());
        s
    }
}

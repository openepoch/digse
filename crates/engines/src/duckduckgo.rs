//! DuckDuckGo search engine implementation
//!
//! Uses the no-JS HTML endpoint (`https://html.duckduckgo.com/html/`). For the first page of results
//! the form only needs the query (`q`), a `kl` region cookie, and an empty `b`;
//! the `vqd` digest is only required to *continue* to later pages, so it is not
//! requested here.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchResult, ResultType, SearchQuery,
};

/// A realistic, static browser User-Agent. DDG's bot blocker rejects obviously
/// non-browser agents (e.g. the default reqwest UA), so a stable Chrome UA is
/// used everywhere.
const USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) \
     Chrome/124.0.0.0 Safari/537.36";

const HTML_ENDPOINT: &str = "https://html.duckduckgo.com/html/";

/// DuckDuckGo search engine
pub struct DuckDuckGoEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl DuckDuckGoEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "duckduckgo".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "DuckDuckGo - Privacy, simplified.".to_string(),
            website: Some("https://duckduckgo.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create DuckDuckGo HTTP client");

        DuckDuckGoEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        // DDG refuses queries longer than 499 characters.
        if query.query.chars().count() >= 500 {
            return Ok(vec![]);
        }

        // First page: `b` is empty and no `vqd` is required. `kl=wt-wt`
        // selects "all regions".
        let form = [("q", query.query.as_str()), ("b", ""), ("kl", "wt-wt")];

        let response = self
            .client
            .post(HTML_ENDPOINT)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .header("Sec-Fetch-Dest", "document")
            .header("Sec-Fetch-Mode", "navigate")
            .header("Sec-Fetch-Site", "same-origin")
            .header("Sec-Fetch-User", "?1")
            .header("Referer", HTML_ENDPOINT)
            .header("Accept-Language", "en-US,en;q=0.7")
            .form(&form)
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(Error::EngineError(
                "duckduckgo".to_string(),
                format!("HTTP error: {}", response.status()),
            ));
        }

        let text = response
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        Ok(self.parse_html(&text, query))
    }

    /// Parse DDG's no-JS HTML into search results.
    ///
    /// Each organic result is `div#links > div.web-result`; ads use the class
    /// `result--ad` and are skipped. The title link (`a.result__a`) points
    /// through a DDG redirect of the form `…/l/?uddg=<encoded real url>&…`,
    /// which we unwrap back to the destination.
    fn parse_html(&self, html: &str, query: &SearchQuery) -> Vec<SearchResult> {
        let document = Html::parse_document(html);

        let result_sel = match Selector::parse("div#links div.web-result, div#links div.result") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let ad_sel = match Selector::parse(".result--ad") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let title_sel = match Selector::parse("a.result__a") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let snippet_sel = match Selector::parse("a.result__snippet, .result__snippet") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let mut results = Vec::new();
        let mut rank = query.offset;

        for result_node in document.select(&result_sel) {
            // Skip advert blocks.
            if result_node.select(&ad_sel).next().is_some() {
                continue;
            }

            let title_elem = match result_node.select(&title_sel).next() {
                Some(e) => e,
                None => continue,
            };

            let title: String = title_elem.text().collect::<String>().trim().to_string();
            let raw_href = title_elem.value().attr("href").unwrap_or("").trim();
            if title.is_empty() || raw_href.is_empty() {
                continue;
            }

            let url = unwrap_ddg_url(raw_href);
            if !url.starts_with("http") {
                continue;
            }

            rank += 1;
            let snippet: Option<String> = result_node
                .select(&snippet_sel)
                .next()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .filter(|s| !s.is_empty());

            let result = SearchResult::new(&title, &url)
                .with_engine("duckduckgo")
                .with_rank(rank)
                .with_score(1.0 - ((rank - query.offset - 1) as f64 * 0.05));
            results.push(if let Some(s) = snippet {
                result.with_snippet(s)
            } else {
                result
            });
        }

        results
    }
}

/// Decode DDG's redirect wrapper back to the destination URL.
///
/// Links in the HTML endpoint look like:
///   //duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2F&rut=…
/// or already be an absolute URL for some results.
fn unwrap_ddg_url(href: &str) -> String {
    // Normalise protocol-relative URLs.
    let href = if let Some(rest) = href.strip_prefix("//") {
        format!("https://{}", rest)
    } else {
        href.to_string()
    };

    if let Some(idx) = href.find("uddg=") {
        let after = &href[idx + "uddg=".len()..];
        let encoded = after.split('&').next().unwrap_or(after);
        return urlencoding::decode(encoded)
            .map(|cow| cow.into_owned())
            .unwrap_or_else(|_| encoded.to_string());
    }

    href
}

#[async_trait]
impl Engine for DuckDuckGoEngine {
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
        *result_type == ResultType::Web || *result_type == ResultType::All
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "https://duckduckgo.com".to_string());
        settings.insert("html_endpoint".to_string(), HTML_ENDPOINT.to_string());
        settings
    }
}

impl Default for DuckDuckGoEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwraps_ddg_redirect_url() {
        let wrapped =
            "//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.rust-lang.org%2F&rut=abc";
        assert_eq!(unwrap_ddg_url(wrapped), "https://www.rust-lang.org/");
    }

    #[test]
    fn passes_through_absolute_url() {
        assert_eq!(
            unwrap_ddg_url("https://example.com/path"),
            "https://example.com/path"
        );
    }

    #[test]
    fn parses_simple_results_html() {
        let html = r##"
        <div id="links">
          <div class="result web-result ">
            <h2 class="result__title">
              <a rel="nofollow" class="result__a"
                 href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.rust-lang.org%2F">
                Rust Programming Language
              </a>
            </h2>
            <a class="result__snippet" href="#">A language empowering everyone.</a>
          </div>
        </div>"##;
        let engine = DuckDuckGoEngine::new();
        let query = SearchQuery::new("rust");
        let results = engine.parse_html(html, &query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Rust Programming Language");
        assert_eq!(results[0].url, "https://www.rust-lang.org/");
        assert_eq!(results[0].engine, "duckduckgo");
        assert_eq!(
            results[0].snippet.as_deref(),
            Some("A language empowering everyone.")
        );
    }
}

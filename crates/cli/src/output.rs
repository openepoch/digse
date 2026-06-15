//! Output formatting for search results

use std::collections::BTreeMap;

use chrono::Utc;
use digse_core::{EngineStatus, SearchQuery, SearchResult, SearchResponse, ResultType};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FormatError {
    #[error("JSON serialization error: {0}")]
    JsonError(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, FormatError>;

/// Trait for output formatters
pub trait Formatter {
    fn format(&self, response: &SearchResponse) -> Result<String>;
}

/// JSON formatter
pub struct JsonFormatter {
    pretty: bool,
}

impl JsonFormatter {
    pub fn new(pretty: bool) -> Self {
        JsonFormatter { pretty }
    }
}

impl Formatter for JsonFormatter {
    fn format(&self, response: &SearchResponse) -> Result<String> {
        if self.pretty {
            Ok(serde_json::to_string_pretty(response)?)
        } else {
            Ok(serde_json::to_string(response)?)
        }
    }
}

/// Build a detailed, self-sufficient JSON envelope around a search response.
///
/// A consumer of `digse search` output gets everything it needs from the single
/// JSON document: the digse version, a generation timestamp, an echo of the
/// request that produced the response, pagination hints, timing, a per-engine
/// status rollup, and the results themselves (each enriched with its domain).
pub fn build_response_envelope(
    response: &SearchResponse,
    request: &SearchQuery,
    version: &str,
) -> serde_json::Value {
    // Roll up per-engine status and results-by-engine from the stats.
    let mut succeeded = 0usize;
    let mut partial = 0usize;
    let mut failed = 0usize;
    let mut timeout = 0usize;
    let mut rate_limited = 0usize;
    let mut results_by_engine: BTreeMap<&str, usize> = BTreeMap::new();
    for stat in &response.engine_stats {
        match stat.status {
            EngineStatus::Success => succeeded += 1,
            EngineStatus::Partial => partial += 1,
            EngineStatus::Failed => failed += 1,
            EngineStatus::Timeout => timeout += 1,
            EngineStatus::RateLimited => rate_limited += 1,
        }
        *results_by_engine.entry(stat.engine.as_str()).or_insert(0) += stat.results_count;
    }
    let engines_queried = response.engine_stats.len();

    // Pagination hints. We don't know the true total available across all
    // engines, so `has_more` is best-effort: a full page implies more may exist.
    let returned = response.results.len();
    let limit = request.count;
    let offset = request.offset;
    let next_offset = offset + returned;
    let has_more = returned >= limit;

    // Per-result enrichment: deserialize each result (which flattens its
    // type-specific `extra` metadata onto the object) and add its domain.
    let results_json: Vec<serde_json::Value> = response
        .results
        .iter()
        .map(|r| {
            let mut obj = serde_json::to_value(r).unwrap_or_else(|_| serde_json::json!({}));
            if let Some(map) = obj.as_object_mut() {
                map.insert("domain".to_string(), serde_json::json!(extract_domain(&r.url)));
            }
            obj
        })
        .collect();

    serde_json::json!({
        "digse": {
            "name": "digse",
            "version": version,
            "generated_at": Utc::now().to_rfc3339(),
        },
        "query": response.query,
        "result_type": response.result_type.as_str(),
        "request": {
            "query": request.query,
            "result_type": request.result_type.as_str(),
            "count": request.count,
            "offset": request.offset,
            "timeout_seconds": request.timeout_seconds,
            "language": request.language,
            "time_range": request.time_range.map(|t| t.as_str()),
            "safe_search": request.safe_search,
        },
        "pagination": {
            "returned": returned,
            "offset": offset,
            "limit": limit,
            "next_offset": next_offset,
            "has_more": has_more,
        },
        "timing": {
            "duration_ms": response.search_duration_ms,
            "per_engine_timeout_ms": request.timeout_seconds.saturating_mul(1000),
        },
        "engines": {
            "queried": engines_queried,
            "used": response.engines_used,
            "summary": {
                "succeeded": succeeded,
                "partial": partial,
                "failed": failed,
                "timeout": timeout,
                "rate_limited": rate_limited,
            },
            "results_by_engine": results_by_engine,
            "stats": response.engine_stats,
        },
        "results": results_json,
        // Convenience counts at the top level.
        "count": returned,
        "total_results": response.total_results,
    })
}

/// Render a JSON value to a string, pretty- or compact-printed.
pub fn render_json(value: &serde_json::Value, pretty: bool) -> Result<String> {
    if pretty {
        Ok(serde_json::to_string_pretty(value)?)
    } else {
        Ok(serde_json::to_string(value)?)
    }
}

/// Extract the registrable host (domain) from a URL.
///
/// Conservative: strips the scheme, then takes everything up to the first `/`,
/// drops any `user@` and `:port`, and lower-cases the result. Returns an empty
/// string for malformed input rather than panicking.
fn extract_domain(url: &str) -> String {
    let after_scheme = url.split("://").nth(1).unwrap_or(url);
    let host = after_scheme.split('/').next().unwrap_or(after_scheme);
    let host = host.split('@').last().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    host.trim().to_lowercase()
}

/// Pretty text formatter
pub struct PrettyFormatter;

impl PrettyFormatter {
    pub fn new() -> Self {
        PrettyFormatter
    }
}

impl Default for PrettyFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl Formatter for PrettyFormatter {
    fn format(&self, response: &SearchResponse) -> Result<String> {
        let mut output = String::new();

        output.push_str(&format!("Query: {}\n", response.query));
        output.push_str(&format!("Type: {}\n", response.result_type.as_str()));
        output.push_str(&format!("Results: {}\n", response.total_results));

        if let Some(duration) = response.search_duration_ms {
            output.push_str(&format!("Time: {}ms\n", duration));
        }

        if !response.engines_used.is_empty() {
            output.push_str(&format!("Engines: {}\n", response.engines_used.join(", ")));
        }

        output.push_str("\nResults:\n");

        for (i, result) in response.results.iter().enumerate() {
            output.push_str(&format!("\n{}. {}\n", i + 1, result.title));
            output.push_str(&format!("   {}\n", result.url));
            if let Some(snippet) = &result.snippet {
                output.push_str(&format!("   {}\n", snippet));
            }
            // Per-type metadata from `extra`.
            for line in format_type_extra(result) {
                output.push_str(&format!("   {}\n", line));
            }
            if !result.engine.is_empty() {
                output.push_str(&format!("   [{}]\n", result.engine));
            }
        }

        Ok(output)
    }
}

/// Render the type-specific `extra` metadata for a result as one compact line
/// per available field, chosen by the result's content type. URL-centric: the
/// result stays a (title, url); this only surfaces the classification metadata.
fn format_type_extra(result: &SearchResult) -> Vec<String> {
    let s = |key: &str| -> Option<String> {
        result
            .extra
            .get(key)
            .and_then(|v| v.as_str())
            .map(|x| x.to_string())
            .filter(|x| !x.is_empty())
    };
    let joined = |keys: &[&str]| -> Option<String> {
        let parts: Vec<String> = keys.iter().filter_map(|k| s(k)).collect();
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" | "))
        }
    };

    match result.result_type {
        ResultType::Images => {
            let mut parts: Vec<String> = Vec::new();
            if let Some(v) = s("img_src") {
                parts.push(format!("img: {}", v));
            }
            if let Some(fmt) = s("format") {
                parts.push(fmt);
            }
            let dims = joined(&["width", "height"]);
            if let Some(d) = dims {
                parts.push(format!("{}px", d));
            }
            if parts.is_empty() {
                vec![]
            } else {
                vec![parts.join("  ")]
            }
        }
        ResultType::Videos => joined(&["duration", "author", "thumbnail", "iframe_src"])
            .map(|l| vec![l])
            .unwrap_or_default(),
        ResultType::Music => joined(&["artist", "album", "duration", "audio_src"])
            .map(|l| vec![l])
            .unwrap_or_default(),
        ResultType::Torrents => {
            let mut parts: Vec<String> = Vec::new();
            if let Some(v) = s("seeders") {
                parts.push(format!("S:{}", v));
            }
            if let Some(v) = s("leechers") {
                parts.push(format!("L:{}", v));
            }
            if let Some(v) = s("filesize") {
                parts.push(v);
            }
            if let Some(v) = s("magnet") {
                parts.push(format!("magnet:{}", &v[..v.len().min(40)]));
            }
            if parts.is_empty() {
                vec![]
            } else {
                vec![parts.join("  ")]
            }
        }
        ResultType::Files => joined(&["filesize", "mimetype", "file_format", "author"])
            .map(|l| vec![l])
            .unwrap_or_default(),
        ResultType::News => joined(&["published", "source", "img_src"])
            .map(|l| vec![l])
            .unwrap_or_default(),
        ResultType::Maps => joined(&["address", "latitude", "longitude", "thumbnail"])
            .map(|l| vec![l])
            .unwrap_or_default(),
        ResultType::Weather => joined(&["temperature", "condition", "humidity", "wind"])
            .map(|l| vec![l])
            .unwrap_or_default(),
        ResultType::Academic => joined(&["doi", "pdf_url", "authors", "published"])
            .map(|l| vec![l])
            .unwrap_or_default(),
        ResultType::IT => joined(&["stars", "forks", "maintainer", "tags"])
            .map(|l| vec![l])
            .unwrap_or_default(),
        _ => vec![],
    }
}

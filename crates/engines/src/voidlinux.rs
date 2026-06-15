//! Void Linux binary packages search engine implementation (JSON).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Void Linux binary packages search engine.
pub struct VoidlinuxEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://xq-api.voidlinux.org";
const PKG_REPO_URL: &str = "https://github.com/void-linux/void-packages";
const DEFAULT_ARCH: &str = "x86_64";

#[derive(Debug, Serialize, Deserialize)]
struct VoidResponse {
    #[serde(default)]
    data: Vec<VoidPackage>,
}

#[derive(Debug, Serialize, Deserialize)]
struct VoidPackage {
    #[serde(default)]
    name: String,
    #[serde(default)]
    short_desc: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    revision: String,
    #[serde(default)]
    filename_size: i64,
    #[serde(default)]
    repository: String,
}

impl VoidlinuxEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "voidlinux".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Void Linux - search binary packages (xq-api.voidlinux.org).".to_string(),
            website: Some("https://voidlinux.org/packages/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Void Linux HTTP client");
        VoidlinuxEngine { metadata, client }
    }

    /// Extract an architecture token from the query if present, returning
    /// `(arch, remaining_query)`.
    fn extract_arch(query: &str) -> (&'static str, String) {
        for arch in [
            "aarch64-musl",
            "armv6l-musl",
            "armv7l-musl",
            "x86_64-musl",
            "aarch64",
            "armv6l",
            "armv7l",
            "i686",
            "x86_64",
        ] {
            if query.contains(arch) {
                return (arch, query.replace(arch, "").trim().to_string());
            }
        }
        (DEFAULT_ARCH, query.to_string())
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let (arch, q) = Self::extract_arch(&query.query);
        let url = format!("{}/v1/query/{}?q={}", BASE_URL, arch, urlencoding::encode(&q));

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: VoidResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        // Merge packages that share the same source-template URL (32bit/dbg).
        let mut packages: std::collections::BTreeMap<String, Vec<&VoidPackage>> =
            std::collections::BTreeMap::new();
        for pkg in parsed.data.iter() {
            // strip "-32bit" and "-dbg" suffixes to derive the github slug
            let slug = strip_suffixes(&pkg.name);
            let pkg_url = format!("{}/tree/master/srcpkgs/{}", PKG_REPO_URL, slug);
            packages.entry(pkg_url).or_default().push(pkg);
        }

        let mut results = Vec::new();
        for (i, (pkg_url, pkg_list)) in packages.into_iter().enumerate() {
            if i >= query.count {
                break;
            }
            let titles: Vec<String> = pkg_list.iter().map(|p| p.name.clone()).collect();
            let names: Vec<String> = titles.clone();
            let first = match pkg_list.first() {
                Some(p) => p,
                None => continue,
            };
            let tags: Vec<String> = pkg_list.iter().map(|p| p.repository.clone()).collect();
            let version = format!("v{}_{}", first.version, first.revision);
            let content = format!(
                "{} - {}",
                first.short_desc,
                humanize_bytes(first.filename_size)
            );

            let r = SearchResult::new(titles.join(" | "), pkg_url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::IT)
                .with_extra("package_name", serde_json::json!(names.join(" | ")))
                .with_extra("version", serde_json::json!(version))
                .with_extra("tags", serde_json::json!(tags))
                .with_extra("arch", serde_json::json!(arch));
            results.push(r);
        }
        Ok(results)
    }
}

fn strip_suffixes(name: &str) -> String {
    let mut n = name.to_string();
    for suffix in ["-32bit", "-dbg"] {
        if n.ends_with(suffix) {
            n = n[..n.len() - suffix.len()].to_string();
        }
    }
    n
}

/// Render a byte count as a human-readable string (e.g. "1.2 MiB").
fn humanize_bytes(bytes: i64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size.abs() >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", size, UNITS[unit])
    }
}

#[async_trait]
impl Engine for VoidlinuxEngine {
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
        matches!(t, ResultType::IT | ResultType::Files | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), BASE_URL.to_string());
        s.insert("pkg_repo_url".to_string(), PKG_REPO_URL.to_string());
        s.insert("void_arch".to_string(), DEFAULT_ARCH.to_string());
        s.insert("results".to_string(), "JSON".to_string());
        s
    }
}

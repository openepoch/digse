//! URL filtering utilities

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

/// URL filter for result filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlFilter {
    /// Allowed file extensions (e.g., "pdf", "jpg", "mp4")
    result_formats: Option<HashSet<String>>,
    /// URL patterns to include
    include_patterns: Option<Vec<String>>,
    /// URL patterns to exclude
    exclude_patterns: Option<Vec<String>>,
}

impl UrlFilter {
    /// Create a new URL filter
    pub fn new() -> Self {
        UrlFilter {
            result_formats: None,
            include_patterns: None,
            exclude_patterns: None,
        }
    }

    /// Check if a URL passes the filter
    pub fn matches(&self, url: &str) -> bool {
        // Check exclude patterns first
        if let Some(exclude) = &self.exclude_patterns {
            for pattern in exclude {
                if self.matches_pattern(url, pattern) {
                    return false;
                }
            }
        }

        // Check include patterns
        if let Some(include) = &self.include_patterns {
            let mut included = false;
            for pattern in include {
                if self.matches_pattern(url, pattern) {
                    included = true;
                    break;
                }
            }
            if !included && !include.is_empty() {
                return false;
            }
        }

        // Check file extensions
        if let Some(formats) = &self.result_formats {
            if !formats.is_empty() {
                let extension = self.get_extension(url);
                return formats.contains(&extension);
            }
        }

        true
    }

    /// Check if URL matches a pattern.
    ///
    /// Patterns support `*` wildcards, each matching any run of characters
    /// (including the empty run). A leading `*` leaves the start unanchored and
    /// a trailing `*` leaves the end unanchored; otherwise that edge must match
    /// exactly. Patterns containing no `*` match as a plain substring.
    fn matches_pattern(&self, url: &str, pattern: &str) -> bool {
        let pattern_lower = pattern.to_lowercase();
        let url_lower = url.to_lowercase();

        if !pattern_lower.contains('*') {
            return url_lower.contains(&pattern_lower);
        }

        // Split into literal segments separated by `*`. Empty segments come from
        // leading/trailing/doubled wildcards and are treated as "match anything".
        let segments: Vec<&str> = pattern_lower.split('*').collect();
        let mut cursor = 0usize;
        for (i, seg) in segments.iter().enumerate() {
            if seg.is_empty() {
                continue;
            }
            if i == 0 {
                // No leading wildcard: anchor the first segment to the start.
                if !url_lower.starts_with(seg) {
                    return false;
                }
                cursor = seg.len();
            } else {
                match url_lower[cursor..].find(seg) {
                    Some(pos) => cursor += pos + seg.len(),
                    None => return false,
                }
            }
        }

        // No trailing wildcard: the last segment must anchor the end of the URL.
        if !pattern_lower.ends_with('*') {
            let last = segments.last().copied().unwrap_or("");
            if !last.is_empty() && !url_lower.ends_with(last) {
                return false;
            }
        }

        true
    }

    /// Get file extension from URL
    fn get_extension(&self, url: &str) -> String {
        let path = Path::new(url);
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase()
    }

    /// Filter a list of URLs
    pub fn filter_urls(&self, urls: &[String]) -> Vec<String> {
        urls.iter()
            .filter(|url| self.matches(url))
            .cloned()
            .collect()
    }
}

impl Default for UrlFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Filter builder
pub struct FilterBuilder {
    result_formats: Option<HashSet<String>>,
    include_patterns: Option<Vec<String>>,
    exclude_patterns: Option<Vec<String>>,
}

impl FilterBuilder {
    pub fn new() -> Self {
        FilterBuilder {
            result_formats: None,
            include_patterns: None,
            exclude_patterns: None,
        }
    }

    /// Set allowed file formats
    pub fn result_formats(mut self, formats: Vec<String>) -> Self {
        self.result_formats = Some(formats.into_iter()
            .map(|f| f.to_lowercase())
            .collect());
        self
    }

    /// Set include patterns
    pub fn include_patterns(mut self, patterns: Vec<String>) -> Self {
        self.include_patterns = Some(patterns);
        self
    }

    /// Set exclude patterns
    pub fn exclude_patterns(mut self, patterns: Vec<String>) -> Self {
        self.exclude_patterns = Some(patterns);
        self
    }

    pub fn build(self) -> UrlFilter {
        UrlFilter {
            result_formats: self.result_formats,
            include_patterns: self.include_patterns,
            exclude_patterns: self.exclude_patterns,
        }
    }
}

impl Default for FilterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_filtering() {
        let filter = FilterBuilder::new()
            .result_formats(vec!["pdf".to_string(), "doc".to_string()])
            .build();

        assert!(filter.matches("https://example.com/document.pdf"));
        assert!(filter.matches("https://example.com/document.doc"));
        assert!(!filter.matches("https://example.com/image.jpg"));
    }

    #[test]
    fn test_pattern_matching() {
        let filter = FilterBuilder::new()
            .include_patterns(vec!["github.com".to_string(), "gitlab.com".to_string()])
            .build();

        assert!(filter.matches("https://github.com/user/repo"));
        assert!(filter.matches("https://gitlab.com/user/repo"));
        assert!(!filter.matches("https://bitbucket.org/user/repo"));
    }

    #[test]
    fn test_wildcard_patterns() {
        let filter = FilterBuilder::new()
            .include_patterns(vec!["*/docs/*".to_string()])
            .build();

        assert!(filter.matches("https://example.com/docs/tutorial"));
        assert!(!filter.matches("https://example.com/guide/tutorial"));
    }

    #[test]
    fn test_exclude_patterns() {
        let filter = FilterBuilder::new()
            .exclude_patterns(vec!["*.exe".to_string(), "*.dll".to_string()])
            .build();

        assert!(!filter.matches("https://example.com/software.exe"));
        assert!(!filter.matches("https://example.com/software.dll"));
        assert!(filter.matches("https://example.com/document.pdf"));
    }
}
//! URL-based post-filters applied to a completed search response, shared by the
//! HTTP `/search` handler. These are the migrated equivalents of the removed
//! `digse search` flags `--result-formats` / `--include-patterns` /
//! `--exclude-patterns` — they run on result URLs after every engine has
//! returned.

/// URL-based post-filters. Keep/drop results by their URL after every engine has
/// returned.
pub struct PostFilters {
    /// Keep only results whose URL path ends with one of these extensions
    /// (e.g. `["pdf", "docx"]`) — the literal "filetype search".
    pub formats: Vec<String>,
    /// Keep only results whose URL contains any of these substrings.
    pub include: Vec<String>,
    /// Drop results whose URL contains any of these substrings.
    pub exclude: Vec<String>,
}

impl PostFilters {
    /// Build from raw comma-separated strings (`None` => that filter is empty).
    pub fn from_raw(formats: Option<&str>, include: Option<&str>, exclude: Option<&str>) -> Self {
        let split = |s: Option<&str>| -> Vec<String> {
            s.map(|raw| {
                raw.split(',')
                    .map(|p| p.trim().to_lowercase())
                    .filter(|p| !p.is_empty())
                    .collect()
            })
            .unwrap_or_default()
        };
        Self {
            formats: split(formats),
            include: split(include),
            exclude: split(exclude),
        }
    }

    /// True if no filter is set (nothing would be dropped).
    pub fn is_empty(&self) -> bool {
        self.formats.is_empty() && self.include.is_empty() && self.exclude.is_empty()
    }
}

/// Apply the filters in place: filetype suffix match, include-any, exclude-any.
/// Resets `total_results` to the surviving count. A no-op when `filters` is
/// empty.
pub fn apply_post_filters(response: &mut digse_core::SearchResponse, filters: &PostFilters) {
    if filters.is_empty() {
        return;
    }
    response.results.retain(|r| {
        let url = r.url.to_lowercase();
        // Filetype: URL must end with one of the extensions.
        if !filters.formats.is_empty() {
            let matches_ext = filters.formats.iter().any(|ext| {
                url.ends_with(&format!(".{}", ext))
                    || url.contains(&format!(".{}?", ext))
                    || url.contains(&format!(".{}#", ext))
            });
            if !matches_ext {
                return false;
            }
        }
        // Include: URL must contain at least one pattern.
        if !filters.include.is_empty() && !filters.include.iter().any(|p| url.contains(p)) {
            return false;
        }
        // Exclude: drop if URL contains any excluded pattern.
        if filters.exclude.iter().any(|p| url.contains(p)) {
            return false;
        }
        true
    });
    response.total_results = response.results.len();
}

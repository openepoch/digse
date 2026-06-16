//! HTML parsing utilities

use scraper::{Html, Selector};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum HtmlParseError {
    #[error("Failed to parse HTML: {0}")]
    ParseError(String),

    #[error("Selector error: {0}")]
    SelectorError(String),

    #[error("Element not found")]
    ElementNotFound,

    #[error("Attribute not found: {0}")]
    AttributeNotFound(String),
}

/// HTML parser for extracting search results
pub struct HtmlParser {
    document: Html,
}

impl HtmlParser {
    /// Create a new HTML parser from HTML string
    pub fn new(html: &str) -> Result<Self, HtmlParseError> {
        let document = Html::parse_document(html);
        Ok(HtmlParser { document })
    }

    /// Create from response text
    pub fn from_response(text: &str) -> Result<Self, HtmlParseError> {
        Self::new(text)
    }

    /// Select elements by CSS selector
    pub fn select(&self, selector: &str) -> Result<HtmlExtractor<'_>, HtmlParseError> {
        let sel = Selector::parse(selector)
            .map_err(|e| HtmlParseError::SelectorError(e.to_string()))?;

        Ok(HtmlExtractor {
            elements: self.document.select(&sel).collect(),
        })
    }

    /// Get text content from selector
    pub fn text(&self, selector: &str) -> Result<String, HtmlParseError> {
        let sel = Selector::parse(selector)
            .map_err(|e| HtmlParseError::SelectorError(e.to_string()))?;

        self.document
            .select(&sel)
            .next()
            .map(|el| el.text().collect::<Vec<_>>().join(" "))
            .ok_or(HtmlParseError::ElementNotFound)
    }
}

/// HTML extractor for working with selected elements
pub struct HtmlExtractor<'a> {
    elements: Vec<scraper::ElementRef<'a>>,
}

impl<'a> HtmlExtractor<'a> {
    /// Get number of elements
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Iterate over elements
    pub fn iter(&self) -> impl Iterator<Item = &scraper::ElementRef<'a>> {
        self.elements.iter()
    }

    /// Get text content from all elements
    pub fn texts(&self) -> Vec<String> {
        self.elements
            .iter()
            .map(|el| el.text().collect::<Vec<_>>().join(" "))
            .collect()
    }

    /// Get attribute from all elements
    pub fn attr(&self, attr_name: &str) -> Vec<Option<String>> {
        self.elements
            .iter()
            .map(|el| el.value().attr(attr_name).map(|s| s.to_string()))
            .collect()
    }

    /// Get inner HTML from all elements
    pub fn inner_html(&self) -> Vec<String> {
        self.elements
            .iter()
            .map(|el| el.inner_html())
            .collect()
    }

    /// Get outer HTML from all elements
    pub fn outer_html(&self) -> Vec<String> {
        self.elements
            .iter()
            .map(|el| el.html())
            .collect()
    }

    /// Select child elements
    pub fn select_children(&self, selector: &str) -> Result<Vec<HtmlExtractor<'_>>, HtmlParseError> {
        let sel = Selector::parse(selector)
            .map_err(|e| HtmlParseError::SelectorError(e.to_string()))?;

        Ok(self.elements
            .iter()
            .map(|el| HtmlExtractor {
                elements: el.select(&sel).collect(),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_parser() {
        let html = r#"
            <html>
                <body>
                    <div class="result">
                        <a href="https://example.com">Example</a>
                        <span class="title">Title</span>
                    </div>
                </body>
            </html>
        "#;

        let parser = HtmlParser::new(html).unwrap();
        let links = parser.select("a").unwrap();

        assert_eq!(links.len(), 1);
        assert_eq!(links.attr("href")[0], Some("https://example.com".to_string()));
    }

    #[test]
    fn test_text_extraction() {
        let html = r#"
            <html>
                <body>
                    <span class="title">Test Title</span>
                </body>
            </html>
        "#;

        let parser = HtmlParser::new(html).unwrap();
        let text = parser.text(".title").unwrap();

        assert_eq!(text, "Test Title");
    }
}
//! Response parsing utilities for HTML and JSON

pub mod html;
pub mod json;

pub use html::{HtmlParser, HtmlExtractor};
pub use json::{JsonParser, JsonExtractor};
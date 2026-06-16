//! JSON parsing utilities

use serde_json::Value;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum JsonParseError {
    #[error("Failed to parse JSON: {0}")]
    ParseError(#[from] serde_json::Error),

    #[error("Key not found: {0}")]
    KeyNotFound(String),

    #[error("Invalid type: expected {expected}, found {found}")]
    InvalidType { expected: String, found: String },

    #[error("Index out of bounds: {0}")]
    IndexOutOfBounds(usize),
}

/// JSON parser for extracting data from JSON responses
pub struct JsonParser {
    value: Value,
}

impl JsonParser {
    /// Create a new JSON parser from JSON string
    pub fn new(json: &str) -> Result<Self, JsonParseError> {
        let value = serde_json::from_str(json)?;
        Ok(JsonParser { value })
    }

    /// Create from response text
    pub fn from_response(text: &str) -> Result<Self, JsonParseError> {
        Self::new(text)
    }

    /// Get value by key path
    pub fn get(&self, path: &str) -> Result<&Value, JsonParseError> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = &self.value;

        for part in parts {
            current = current.get(part).ok_or_else(|| JsonParseError::KeyNotFound(part.to_string()))?;
        }

        Ok(current)
    }

    /// Get string value by key path
    pub fn get_string(&self, path: &str) -> Result<String, JsonParseError> {
        let value = self.get(path)?;

        value.as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| JsonParseError::InvalidType {
                expected: "string".to_string(),
                found: format!("{:?}", value),
            })
    }

    /// Get array by key path
    pub fn get_array(&self, path: &str) -> Result<&Vec<Value>, JsonParseError> {
        let value = self.get(path)?;

        value.as_array()
            .ok_or_else(|| JsonParseError::InvalidType {
                expected: "array".to_string(),
                found: format!("{:?}", value),
            })
    }

    /// Get object by key path
    pub fn get_object(&self, path: &str) -> Result<&serde_json::Map<String, Value>, JsonParseError> {
        let value = self.get(path)?;

        value.as_object()
            .ok_or_else(|| JsonParseError::InvalidType {
                expected: "object".to_string(),
                found: format!("{:?}", value),
            })
    }

    /// Extract array of objects
    pub fn extract_objects(&self, path: &str) -> Result<Vec<JsonExtractor<'_>>, JsonParseError> {
        let array = self.get_array(path)?;
        Ok(array
            .iter()
            .map(|v| JsonExtractor { value: v })
            .collect())
    }

    /// Get root value
    pub fn root(&self) -> &Value {
        &self.value
    }
}

/// JSON extractor for working with specific values
pub struct JsonExtractor<'a> {
    value: &'a Value,
}

impl<'a> JsonExtractor<'a> {
    /// Get string field
    pub fn get_string(&self, key: &str) -> Result<String, JsonParseError> {
        self.value
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| JsonParseError::KeyNotFound(key.to_string()))
    }

    /// Get optional string field
    pub fn get_string_optional(&self, key: &str) -> Option<String> {
        self.value
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Get number field
    pub fn get_number(&self, key: &str) -> Result<f64, JsonParseError> {
        self.value
            .get(key)
            .and_then(|v| v.as_f64())
            .ok_or_else(|| JsonParseError::InvalidType {
                expected: "number".to_string(),
                found: format!("{:?}", self.value.get(key)),
            })
    }

    /// Get boolean field
    pub fn get_bool(&self, key: &str) -> Result<bool, JsonParseError> {
        self.value
            .get(key)
            .and_then(|v| v.as_bool())
            .ok_or_else(|| JsonParseError::InvalidType {
                expected: "boolean".to_string(),
                found: format!("{:?}", self.value.get(key)),
            })
    }

    /// Get nested object
    pub fn get_object(&self, key: &str) -> Result<JsonExtractor<'_>, JsonParseError> {
        self.value
            .get(key)
            .and_then(|v| v.as_object())
            .map(|_| JsonExtractor { value: self.value })
            .ok_or_else(|| JsonParseError::InvalidType {
                expected: "object".to_string(),
                found: format!("{:?}", self.value.get(key)),
            })
    }

    /// Get array field
    pub fn get_array(&self, key: &str) -> Result<Vec<JsonExtractor<'_>>, JsonParseError> {
        self.value
            .get(key)
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().map(|v| JsonExtractor { value: v }).collect())
            .ok_or_else(|| JsonParseError::InvalidType {
                expected: "array".to_string(),
                found: format!("{:?}", self.value.get(key)),
            })
    }

    /// Get all keys
    pub fn keys(&self) -> Vec<String> {
        self.value
            .as_object()
            .map(|obj| obj.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Check if key exists
    pub fn has_key(&self, key: &str) -> bool {
        self.value.get(key).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_parser() {
        let json = r#"
            {
                "results": [
                    {
                        "title": "Test Title",
                        "url": "https://example.com"
                    }
                ]
            }
        "#;

        let parser = JsonParser::new(json).unwrap();
        let results = parser.extract_objects("results").unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].get_string("title").unwrap(), "Test Title");
        assert_eq!(results[0].get_string("url").unwrap(), "https://example.com");
    }

    #[test]
    fn test_key_path() {
        let json = r#"
            {
                "data": {
                    "query": "test"
                }
            }
        "#;

        let parser = JsonParser::new(json).unwrap();
        let query = parser.get_string("data.query").unwrap();

        assert_eq!(query, "test");
    }
}
// Date and time handling functions
// Mirrors datetime.js from the reference implementation

use chrono::{DateTime, Utc};
use serde_json::Value;
use thiserror::Error;

/// DateTime errors
#[derive(Error, Debug)]
pub enum DateTimeError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Format error: {0}")]
    FormatError(String),
}

/// Parse an ISO 8601 datetime string
pub fn parse_iso8601(s: &str) -> Result<DateTime<Utc>, DateTimeError> {
    s.parse::<DateTime<Utc>>()
        .map_err(|e| DateTimeError::ParseError(e.to_string()))
}

/// Format a datetime as ISO 8601 string
pub fn format_iso8601(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

/// $now() - Get current timestamp
pub fn now() -> Value {
    let now = Utc::now();
    Value::String(format_iso8601(&now))
}

/// $millis() - Get milliseconds since epoch
pub fn millis() -> Value {
    let now = Utc::now();
    Value::Number(now.timestamp_millis().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_now() {
        let result = now();
        assert!(matches!(result, Value::String(_)));
    }

    #[test]
    fn test_millis() {
        let result = millis();
        assert!(matches!(result, Value::Number(_)));
    }
}

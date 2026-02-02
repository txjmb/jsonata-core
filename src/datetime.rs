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
/// Uses 'Z' suffix for UTC timezone (not '+00:00') to match JavaScript behavior
pub fn format_iso8601(dt: &DateTime<Utc>) -> String {
    use chrono::SecondsFormat;
    dt.to_rfc3339_opts(SecondsFormat::Millis, true)
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

/// $toMillis(timestamp) - Convert ISO 8601 timestamp to milliseconds since epoch
pub fn to_millis(timestamp: &str) -> Result<Value, DateTimeError> {
    let dt = parse_iso8601(timestamp)?;
    Ok(Value::Number(dt.timestamp_millis().into()))
}

/// $fromMillis(millis) - Convert milliseconds since epoch to ISO 8601 timestamp
pub fn from_millis(millis: i64) -> Result<Value, DateTimeError> {
    use chrono::TimeZone;

    let dt = Utc.timestamp_millis_opt(millis)
        .single()
        .ok_or_else(|| DateTimeError::FormatError(format!("Invalid timestamp: {}", millis)))?;

    Ok(Value::String(format_iso8601(&dt)))
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

    #[test]
    fn test_to_millis() {
        let result = to_millis("1970-01-01T00:00:00.001Z").unwrap();
        assert_eq!(result, Value::Number(1.into()));
    }

    #[test]
    fn test_from_millis() {
        let result = from_millis(1).unwrap();
        assert!(matches!(result, Value::String(_)));
    }
}

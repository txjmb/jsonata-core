// Date and time handling functions
// Mirrors datetime.js from the reference implementation

use std::sync::OnceLock;

use crate::value::JValue;
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Timelike, Utc};
use regex::Regex;
use thiserror::Error;

/// Compiled regex for ISO 8601 partial format parsing (compiled once on first use)
fn iso8601_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(\d{4})(?:-(\d{2}))?(?:-(\d{2}))?(?:T(\d{2}):(\d{2}):(\d{2}))?(?:\.(\d+))?(Z|[+-]\d{2}:?\d{2})?$"
        ).unwrap()
    })
}

/// DateTime errors
#[derive(Error, Debug)]
pub enum DateTimeError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Format error: {0}")]
    FormatError(String),
}

/// Parse an ISO 8601 datetime string (full format)
#[allow(dead_code)]
pub fn parse_iso8601(s: &str) -> Result<DateTime<Utc>, DateTimeError> {
    s.parse::<DateTime<Utc>>()
        .map_err(|e| DateTimeError::ParseError(e.to_string()))
}

/// Parse a potentially partial ISO 8601 timestamp
/// Supports: YYYY, YYYY-MM, YYYY-MM-DD, YYYY-MM-DDTHH:MM:SS, etc.
pub fn parse_iso8601_partial(s: &str) -> Result<i64, DateTimeError> {
    if let Some(caps) = iso8601_regex().captures(s) {
        let year: i32 = caps.get(1).unwrap().as_str().parse().unwrap();
        let month: u32 = caps.get(2).map_or(1, |m| m.as_str().parse().unwrap());
        let day: u32 = caps.get(3).map_or(1, |m| m.as_str().parse().unwrap());
        let hour: u32 = caps.get(4).map_or(0, |m| m.as_str().parse().unwrap());
        let minute: u32 = caps.get(5).map_or(0, |m| m.as_str().parse().unwrap());
        let second: u32 = caps.get(6).map_or(0, |m| m.as_str().parse().unwrap());
        let millis: u32 = caps.get(7).map_or(0, |m| {
            let s = m.as_str();
            // Pad or truncate to 3 digits for milliseconds
            let padded = format!("{:0<3}", &s[..s.len().min(3)]);
            padded.parse().unwrap_or(0)
        });

        // Parse timezone offset
        let tz_offset_minutes: i32 = caps.get(8).map_or(0, |m| {
            let tz = m.as_str();
            if tz == "Z" {
                0
            } else {
                // Parse +HH:MM or +HHMM format
                let sign = if tz.starts_with('-') { -1 } else { 1 };
                let digits: String = tz[1..].chars().filter(|c| c.is_ascii_digit()).collect();
                if digits.len() >= 4 {
                    let hours: i32 = digits[0..2].parse().unwrap_or(0);
                    let mins: i32 = digits[2..4].parse().unwrap_or(0);
                    sign * (hours * 60 + mins)
                } else if digits.len() >= 2 {
                    let hours: i32 = digits[0..2].parse().unwrap_or(0);
                    sign * hours * 60
                } else {
                    0
                }
            }
        });

        // Create UTC datetime
        let naive = NaiveDate::from_ymd_opt(year, month, day)
            .and_then(|d| d.and_hms_milli_opt(hour, minute, second, millis))
            .ok_or_else(|| DateTimeError::ParseError(format!("Invalid date components: {}", s)))?;

        let utc_dt = Utc.from_utc_datetime(&naive);
        let millis = utc_dt.timestamp_millis() - (tz_offset_minutes as i64 * 60 * 1000);

        Ok(millis)
    } else {
        Err(DateTimeError::ParseError(
            "premature end of input".to_string(),
        ))
    }
}

/// Format a datetime as ISO 8601 string
/// Uses 'Z' suffix for UTC timezone (not '+00:00') to match JavaScript behavior
pub fn format_iso8601(dt: &DateTime<Utc>) -> String {
    use chrono::SecondsFormat;
    dt.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// $now() - Get current timestamp
pub fn now() -> JValue {
    let now = Utc::now();
    JValue::string(format_iso8601(&now))
}

/// $millis() - Get milliseconds since epoch
pub fn millis() -> JValue {
    let now = Utc::now();
    JValue::Number(now.timestamp_millis() as f64)
}

/// $toMillis(timestamp) - Convert ISO 8601 timestamp to milliseconds since epoch
pub fn to_millis(timestamp: &str) -> Result<JValue, DateTimeError> {
    let millis = parse_iso8601_partial(timestamp)?;
    Ok(JValue::Number(millis as f64))
}

/// $toMillis(timestamp, picture) - Convert formatted timestamp to milliseconds since epoch
/// Supports format descriptors like [Y0001][M01][D01]
pub fn to_millis_with_picture(timestamp: &str, picture: &str) -> Result<JValue, DateTimeError> {
    let millis = parse_datetime_with_picture(timestamp, picture)?;
    Ok(JValue::Number(millis as f64))
}

/// Parse a datetime string using a format picture
/// Format descriptors follow XPath/XSLT conventions:
/// [Y] - Year
/// [M] - Month
/// [D] - Day
/// [H] - Hour (24h)
/// [h] - Hour (12h)
/// [m] - Minute
/// [s] - Second
/// [f] - Fractional seconds
/// [P] - AM/PM
/// [Z] - Timezone
pub fn parse_datetime_with_picture(timestamp: &str, picture: &str) -> Result<i64, DateTimeError> {
    // Parse the picture string into components
    let components = analyse_picture(picture)?;

    // Build a regex from the components and extract values
    let parsed = parse_with_components(timestamp, &components)?;

    // Default missing values
    let year = parsed.year.unwrap_or(1970);
    let month = parsed.month.unwrap_or(1);
    let day = parsed.day.unwrap_or(1);
    let hour = parsed.hour.unwrap_or(0);
    let minute = parsed.minute.unwrap_or(0);
    let second = parsed.second.unwrap_or(0);
    let millis = parsed.millis.unwrap_or(0);

    // Handle 12-hour format with AM/PM
    let hour = match parsed.period {
        Some(1) => {
            if hour == 12 {
                12
            } else {
                hour + 12
            }
        } // PM
        Some(_) => {
            if hour == 12 {
                0
            } else {
                hour
            }
        } // AM
        None => hour,
    };

    // Create UTC datetime
    let naive = NaiveDate::from_ymd_opt(year, month, day)
        .and_then(|d| d.and_hms_milli_opt(hour, minute, second, millis))
        .ok_or_else(|| DateTimeError::ParseError("Invalid date components".to_string()))?;

    let utc_dt = Utc.from_utc_datetime(&naive);
    let millis = utc_dt.timestamp_millis() - (parsed.tz_offset.unwrap_or(0) as i64 * 60 * 1000);

    Ok(millis)
}

#[derive(Debug, Clone)]
struct PictureComponent {
    component: char,
    min_width: usize,
    max_width: usize,
}

#[derive(Debug, Default)]
struct ParsedDateTime {
    year: Option<i32>,
    month: Option<u32>,
    day: Option<u32>,
    hour: Option<u32>,
    minute: Option<u32>,
    second: Option<u32>,
    millis: Option<u32>,
    period: Option<u32>,    // 0=AM, 1=PM
    tz_offset: Option<i32>, // minutes
}

/// Analyse a picture string into its components
fn analyse_picture(picture: &str) -> Result<Vec<PictureComponent>, DateTimeError> {
    let mut components = Vec::new();
    let mut chars = picture.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '[' {
            // Start of a component
            let component_char = chars.next().ok_or_else(|| {
                DateTimeError::ParseError("Unexpected end of picture".to_string())
            })?;

            // Parse optional width specifiers
            let mut width_spec = String::new();
            while let Some(&ch) = chars.peek() {
                if ch == ']' {
                    chars.next();
                    break;
                }
                width_spec.push(chars.next().unwrap());
            }

            // Parse width from specifier (e.g., "0001" means 4 digits)
            let (min_width, max_width) = parse_width_spec(&width_spec);

            components.push(PictureComponent {
                component: component_char,
                min_width,
                max_width,
            });
        }
        // Ignore literal characters between components
    }

    Ok(components)
}

/// Parse width specification from format descriptors
/// Examples: "0001" -> (4, 4), "01" -> (2, 2), "" -> (1, max)
fn parse_width_spec(spec: &str) -> (usize, usize) {
    // Handle comma-separated width ranges like "Y,*-4"
    if spec.contains(',') {
        let parts: Vec<&str> = spec.split(',').collect();
        if parts.len() >= 2 {
            let range = parts[1];
            if range.contains('-') {
                // Parse range like "*-4"
                let range_parts: Vec<&str> = range.split('-').collect();
                let max = range_parts
                    .get(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(usize::MAX);
                return (1, max);
            }
        }
    }

    // Count leading zeros to determine minimum width
    let zeros: usize = spec.chars().take_while(|&c| c == '0').count();
    let width = zeros
        + spec
            .chars()
            .filter(|c| c.is_ascii_digit() && *c != '0')
            .count();

    if width == 0 {
        (1, usize::MAX)
    } else {
        (width, width)
    }
}

/// Parse a timestamp using the analysed picture components
fn parse_with_components(
    timestamp: &str,
    components: &[PictureComponent],
) -> Result<ParsedDateTime, DateTimeError> {
    let mut result = ParsedDateTime::default();
    let mut pos = 0;
    let chars: Vec<char> = timestamp.chars().collect();

    for comp in components {
        // Extract digits or characters based on component type
        let end = (pos + comp.max_width).min(chars.len());

        // For numeric components, extract digits
        let value_str: String = chars[pos..end]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .collect();

        if value_str.is_empty() {
            return Err(DateTimeError::ParseError(
                "input contains invalid characters".to_string(),
            ));
        }

        let value: u32 = value_str
            .parse()
            .map_err(|_| DateTimeError::ParseError("Invalid number".to_string()))?;

        pos += value_str.len().max(comp.min_width);

        match comp.component {
            'Y' => result.year = Some(value as i32),
            'M' => result.month = Some(value),
            'D' => result.day = Some(value),
            'H' => result.hour = Some(value),
            'h' => result.hour = Some(value),
            'm' => result.minute = Some(value),
            's' => result.second = Some(value),
            'f' => result.millis = Some(value),
            'P' => result.period = Some(value),
            'Z' | 'z' => result.tz_offset = Some(value as i32),
            _ => {}
        }
    }

    Ok(result)
}

/// $fromMillis(millis) - Convert milliseconds since epoch to ISO 8601 timestamp
pub fn from_millis(millis: i64) -> Result<JValue, DateTimeError> {
    let dt = Utc
        .timestamp_millis_opt(millis)
        .single()
        .ok_or_else(|| DateTimeError::FormatError(format!("Invalid timestamp: {}", millis)))?;

    Ok(JValue::string(format_iso8601(&dt)))
}

/// $fromMillis(millis, picture) - Convert milliseconds to formatted timestamp
#[allow(dead_code)]
pub fn from_millis_with_picture(millis: i64, picture: &str) -> Result<JValue, DateTimeError> {
    let dt = Utc
        .timestamp_millis_opt(millis)
        .single()
        .ok_or_else(|| DateTimeError::FormatError(format!("Invalid timestamp: {}", millis)))?;

    let formatted = format_datetime_with_picture(&dt, picture)?;
    Ok(JValue::string(formatted))
}

/// Format a datetime using a picture string
#[allow(dead_code)]
pub fn format_datetime_with_picture(
    dt: &DateTime<Utc>,
    picture: &str,
) -> Result<String, DateTimeError> {
    let components = analyse_picture(picture)?;
    let mut result = String::new();

    for comp in components {
        let value = match comp.component {
            'Y' => format!("{:0width$}", dt.year(), width = comp.min_width),
            'M' => format!("{:0width$}", dt.month(), width = comp.min_width),
            'D' => format!("{:0width$}", dt.day(), width = comp.min_width),
            'H' => format!("{:0width$}", dt.hour(), width = comp.min_width),
            'h' => {
                let h = dt.hour() % 12;
                format!(
                    "{:0width$}",
                    if h == 0 { 12 } else { h },
                    width = comp.min_width
                )
            }
            'm' => format!("{:0width$}", dt.minute(), width = comp.min_width),
            's' => format!("{:0width$}", dt.second(), width = comp.min_width),
            'f' => {
                let ms = dt.timestamp_subsec_millis();
                format!("{:0width$}", ms, width = comp.min_width)
            }
            'P' => {
                if dt.hour() < 12 {
                    "am".to_string()
                } else {
                    "pm".to_string()
                }
            }
            'Z' => "Z".to_string(),
            _ => String::new(),
        };
        result.push_str(&value);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_now() {
        let result = now();
        assert!(matches!(result, JValue::String(_)));
    }

    #[test]
    fn test_millis() {
        let result = millis();
        assert!(matches!(result, JValue::Number(_)));
    }

    #[test]
    fn test_to_millis() {
        let result = to_millis("1970-01-01T00:00:00.001Z").unwrap();
        assert_eq!(result, JValue::Number(1.0));
    }

    #[test]
    fn test_to_millis_partial_date() {
        // Test date only
        let result = to_millis("2017-10-30").unwrap();
        assert_eq!(result, JValue::Number(1509321600000_i64 as f64));

        // Test year only
        let result = to_millis("2018").unwrap();
        assert_eq!(result, JValue::Number(1514764800000_i64 as f64));
    }

    #[test]
    fn test_to_millis_with_picture() {
        // Test custom format
        let result = to_millis_with_picture("201802", "[Y0001][M01]").unwrap();
        assert_eq!(result, JValue::Number(1517443200000_i64 as f64));

        // Test full date format
        let result = to_millis_with_picture("20180205", "[Y0001][M01][D01]").unwrap();
        assert_eq!(result, JValue::Number(1517788800000_i64 as f64));
    }

    #[test]
    fn test_from_millis() {
        let result = from_millis(1).unwrap();
        assert!(matches!(result, JValue::String(_)));
    }
}

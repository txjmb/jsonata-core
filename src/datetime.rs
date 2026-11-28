// Date and time handling functions
// Mirrors datetime.js from the reference implementation

#![allow(clippy::unnecessary_cast)]

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::value::JValue;
use regex::Regex;
use thiserror::Error;

/// Compiled regex for ISO 8601 partial format parsing (compiled once on first use)
fn iso8601_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(\d{4})(?:-([01]\d))?(?:-([0-3]\d))?(?:T([0-2]\d):([0-5]\d):([0-5]\d))?(?:\.(\d+))?(Z|[+-][0-2]\d:?[0-5]\d)?$"
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

    /// A JSONata-spec-coded error (D3130-D3136 for the datetime/integer picture-string
    /// engine). The code is placed at the start of the message (matching the convention
    /// used elsewhere in this codebase, e.g. "D3120: ..." in evaluator.rs) so that
    /// callers extracting an error code via a `^([TDUS]\d{4}):` prefix match find it.
    #[error("{code}: {message}")]
    Coded { code: &'static str, message: String },
}

fn coded(code: &'static str, message: impl Into<String>) -> DateTimeError {
    DateTimeError::Coded {
        code,
        message: message.into(),
    }
}

/// Parse a potentially partial ISO 8601 timestamp
/// Supports: YYYY, YYYY-MM, YYYY-MM-DD, YYYY-MM-DDTHH:MM:SS, etc.
pub fn parse_iso8601_partial(s: &str) -> Result<f64, DateTimeError> {
    // jsonata-js checks the timestamp against this pattern (D3110 if it does
    // not match) and returns `Date.parse(timestamp)`. `Date.parse` reads a
    // day up to 31 in any month and carries it over (30 February is 2
    // March), allows 24:00:00 as the next midnight, and gives NaN for a month
    // or day of 0, a month over 12, a day over 31, any other hour past 23, an
    // offset of 24 hours or more, and (in V8) a fraction or a numeric offset
    // with no time.
    //
    // Two differences remain, both down to the JavaScript engine rather than
    // jsonata-js. A date-time with no offset is read as UTC here; `Date.parse`
    // reads it in the host's time zone, which is the same only on a host set
    // to UTC. And V8's fallback parser reads "2018.5" as 1 May and
    // "2018-01.5" as 5 January (local time); both are NaN here.
    let Some(caps) = iso8601_regex().captures(s) else {
        return Err(DateTimeError::ParseError(
            "premature end of input".to_string(),
        ));
    };
    let field = |i: usize, default: i64| {
        caps.get(i)
            .map_or(default, |m| m.as_str().parse().unwrap_or(default))
    };
    let year = field(1, 0);
    let month = field(2, 1);
    let day = field(3, 1);
    let hour = field(4, 0);
    let minute = field(5, 0);
    let second = field(6, 0);
    let millis: i64 = caps.get(7).map_or(0, |m| {
        // The first three digits, padded: ".5" is 500 ms.
        let digits = m.as_str();
        format!("{:0<3}", &digits[..digits.len().min(3)])
            .parse()
            .unwrap_or(0)
    });
    // Parse timezone offset: Z, +HH:MM or +HHMM.
    let (offset_hours, offset_minutes) = match caps.get(8).map(|m| m.as_str()) {
        None | Some("Z") => (0, 0),
        Some(tz) => {
            let sign = if tz.starts_with('-') { -1 } else { 1 };
            let digits: String = tz[1..].chars().filter(|c| c.is_ascii_digit()).collect();
            let hours: i64 = digits[0..2].parse().unwrap_or(0);
            let minutes: i64 = digits[2..4].parse().unwrap_or(0);
            (sign * hours, sign * minutes)
        }
    };

    let has_time = caps.get(4).is_some();
    let numeric_offset = caps.get(8).is_some_and(|m| m.as_str() != "Z");
    let midnight_next = hour == 24 && minute == 0 && second == 0 && millis == 0;
    if (!has_time && (caps.get(7).is_some() || numeric_offset))
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(hour < 24 || midnight_next)
        || offset_hours.abs() > 23
    {
        return Ok(f64::NAN);
    }
    let days = days_from_civil(year, month, 1) + day - 1;
    let utc_millis = days * MILLIS_PER_DAY + ((hour * 60 + minute) * 60 + second) * 1000 + millis
        - (offset_hours * 60 + offset_minutes) * 60 * 1000;
    // A four-digit year is well inside JavaScript's dates, so no clip is due.
    Ok(utc_millis as f64)
}

/// The clock as milliseconds since the Unix epoch.
fn clock_millis() -> i64 {
    // `SystemTime` before the epoch is an error; its distance back is the
    // (negative) answer.
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(after) => after.as_millis() as i64,
        Err(before) => -(before.duration().as_millis() as i64),
    }
}

/// The milliseconds in a day.
const MILLIS_PER_DAY: i64 = 86_400_000;

/// The furthest a JavaScript `Date` reaches either side of the epoch, in
/// milliseconds (ECMAScript's TimeClip): 100,000,000 days.
const MAX_DATE_MILLIS: i64 = 8_640_000_000_000_000;

/// The furthest year `date_utc` builds.
const MAX_CIVIL_YEAR: i64 = 1_000_000;

/// A day in the proleptic Gregorian calendar, the calendar JavaScript's `Date`
/// uses, held as its distance in days from 1970-01-01. The conversions to and
/// from year, month and day are Howard Hinnant's `days_from_civil` and
/// `civil_from_days` (<https://howardhinnant.github.io/date_algorithms.html>),
/// exact for every `i64` day this code can produce.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CivilDate {
    days: i64,
}

impl CivilDate {
    /// The date `millis` falls on, counting from the epoch in UTC.
    fn from_millis(millis: i64) -> Self {
        Self {
            days: millis.div_euclid(MILLIS_PER_DAY),
        }
    }

    /// (year, month 1-12, day 1-31).
    fn ymd(self) -> (i64, i64, i64) {
        let z = self.days + 719_468;
        let era = z.div_euclid(146_097);
        let day_of_era = z - era * 146_097;
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let month_from_march = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
        let month = if month_from_march < 10 {
            month_from_march + 3
        } else {
            month_from_march - 9
        };
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        (year, month, day)
    }

    fn year(self) -> i64 {
        self.ymd().0
    }

    /// The month, 0-11.
    fn month0(self) -> i64 {
        self.ymd().1 - 1
    }

    /// The day of the week, Monday 1 to Sunday 7. 1970-01-01 was a Thursday.
    fn weekday_from_monday(self) -> i64 {
        (self.days + 3).rem_euclid(7) + 1
    }

    fn plus_days(self, days: i64) -> Self {
        Self {
            days: self.days + days,
        }
    }
}

/// Days from 1970-01-01 to `year`-`month`-`day` (Hinnant's
/// `days_from_civil`), for a month 1-12 and a year within `MAX_CIVIL_YEAR`.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let year_of_era = y - era * 400;
    let month_from_march = (month + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

thread_local! {
    /// The timestamp of the evaluation in progress on this thread, if any.
    static EVALUATION_TIMESTAMP: std::cell::Cell<Option<i64>> =
        const { std::cell::Cell::new(None) };
}

/// One evaluation's timestamp, held for as long as the scope lives.
///
/// jsonata-js reads the clock once when `evaluate()` starts
/// (`environment.timestamp = new Date()`), and every `$now()` and `$millis()`
/// in that evaluation reports it, as does `$toMillis` for the components a
/// picture leaves out, so they agree with each other (#294). An evaluation
/// nested inside another (a host function evaluating an expression of its
/// own) gets its own timestamp, and the outer one's comes back when it ends.
///
/// jsonata-js keeps it in the evaluation's environment. It is a thread-local
/// here because the builtins that read it are dispatched without the
/// evaluator, and an evaluation runs on one thread from start to end.
pub(crate) struct EvaluationTimestamp {
    outer: Option<i64>,
}

impl EvaluationTimestamp {
    pub(crate) fn enter() -> Self {
        let outer = EVALUATION_TIMESTAMP.with(|t| t.replace(Some(clock_millis())));
        Self { outer }
    }
}

impl Drop for EvaluationTimestamp {
    fn drop(&mut self) {
        EVALUATION_TIMESTAMP.with(|t| t.set(self.outer));
    }
}

/// The timestamp of the evaluation in progress, or the clock outside one.
pub(crate) fn evaluation_millis() -> i64 {
    EVALUATION_TIMESTAMP
        .with(|t| t.get())
        .unwrap_or_else(clock_millis)
}

/// $now() - The evaluation's timestamp, as an ISO 8601 string
pub fn now() -> JValue {
    // `$now()` is `$fromMillis()` of the timestamp, whose default picture is
    // ISO 8601; a clock beyond JavaScript's dates is the only way it fails.
    from_millis_with_picture(evaluation_millis() as f64, None, None).unwrap_or(JValue::Undefined)
}

/// $millis() - The evaluation's timestamp, in milliseconds since the epoch
pub fn millis() -> JValue {
    JValue::Number(evaluation_millis() as f64)
}

/// $toMillis(timestamp) - Convert ISO 8601 timestamp to milliseconds since epoch
pub fn to_millis(timestamp: &str) -> Result<JValue, DateTimeError> {
    // The reference reports one error for anything the ISO 8601 parser cannot
    // read, naming the offending value; the parser's own complaint about
    // *where* it gave up is an implementation detail with no code attached.
    let millis = parse_iso8601_partial(timestamp).map_err(|_| {
        coded(
            "D3110",
            format!(
                "The argument of the toMillis function must be an ISO 8601 formatted timestamp. Given \"{}\"",
                timestamp
            ),
        )
    })?;
    Ok(JValue::Number(millis))
}

/// $toMillis(timestamp, picture) - Convert formatted timestamp to milliseconds since epoch
pub fn to_millis_with_picture(timestamp: &str, picture: &str) -> Result<JValue, DateTimeError> {
    match parse_date_time(timestamp, picture, evaluation_millis())? {
        Some(millis) => Ok(JValue::Number(millis)),
        None => Ok(JValue::Undefined),
    }
}

/// $fromMillis(millis, picture?, timezone?) - Convert milliseconds since epoch to a
/// formatted timestamp. `picture` defaults to ISO 8601; `timezone` defaults to UTC.
pub fn from_millis_with_picture(
    millis: f64,
    picture: Option<&str>,
    timezone: Option<&str>,
) -> Result<JValue, DateTimeError> {
    let formatted = format_date_time(millis, picture, timezone)?;
    Ok(JValue::string(formatted))
}

/// $formatInteger(value, picture) - Format a number using a `fn:format-integer` picture
/// string (roman numerals, spreadsheet letters, spelled-out words, decimal-digit
/// patterns with grouping separators, ordinals).
pub fn format_integer(value: f64, picture: &str) -> Result<JValue, DateTimeError> {
    let format = analyse_integer_picture(picture)?;
    let formatted = format_integer_value(value.floor(), &format)?;
    Ok(JValue::string(formatted))
}

/// $parseInteger(value, picture) - Parse a string formatted per a `fn:format-integer`
/// picture string back into a number. Note (matching the JS reference implementation):
/// the input is not validated against the picture's shape before parsing -- an
/// unparseable value simply fails at the underlying number/word/numeral conversion.
pub fn parse_integer(value: &str, picture: &str) -> Result<JValue, DateTimeError> {
    let format = analyse_integer_picture(picture)?;
    // A malformed *picture* still raises (D3130), as it does in jsonata-js.
    // A value that does not match does not: like the reference, which parses
    // without validating the input first, the answer is whatever its matcher's
    // `parse` gives -- JavaScript's `parseInt` for digits, which reads a
    // leading run of digits and gives NaN when there is none, so
    // `$parseInteger("12a", "000")` is 12 and `$parseInteger("abc", "000")` is
    // NaN rather than a D3136 error (#126 group 3); NaN for unknown words.
    if format.primary == PrimaryFormat::Sequence {
        return Err(coded("D3130", format.token.clone().unwrap_or_default()));
    }
    let parsed = parse_integer_format_value(&format, value);
    Ok(JValue::Number(parsed))
}

// ═══════════════════════════════════════════════════════════════════════════
// §9.8.4 XPath fn:format-integer / fn:format-dateTime picture-string engine
// Mirrors datetime.js's IIFE body (jsonata-js reference implementation).
// ═══════════════════════════════════════════════════════════════════════════

// ── Number-to-words / words-to-number ────────────────────────────────────────

const FEW: [&str; 20] = [
    "Zero",
    "One",
    "Two",
    "Three",
    "Four",
    "Five",
    "Six",
    "Seven",
    "Eight",
    "Nine",
    "Ten",
    "Eleven",
    "Twelve",
    "Thirteen",
    "Fourteen",
    "Fifteen",
    "Sixteen",
    "Seventeen",
    "Eighteen",
    "Nineteen",
];
const ORDINALS: [&str; 20] = [
    "Zeroth",
    "First",
    "Second",
    "Third",
    "Fourth",
    "Fifth",
    "Sixth",
    "Seventh",
    "Eighth",
    "Ninth",
    "Tenth",
    "Eleventh",
    "Twelfth",
    "Thirteenth",
    "Fourteenth",
    "Fifteenth",
    "Sixteenth",
    "Seventeenth",
    "Eighteenth",
    "Nineteenth",
];
const DECADES: [&str; 9] = [
    "Twenty", "Thirty", "Forty", "Fifty", "Sixty", "Seventy", "Eighty", "Ninety", "Hundred",
];
const MAGNITUDES: [&str; 4] = ["Thousand", "Million", "Billion", "Trillion"];

// Uses f64 throughout (matching JS's Number type) rather than i64: $formatInteger/
// $parseInteger with the 'w' picture must handle magnitudes far beyond i64::MAX (e.g.
// 1e46, "ten billion trillion trillion trillion") since English number words are
// unbounded. The magnitude clamp to MAGNITUDES.len() keeps recursion depth and the
// literal word list finite regardless.
fn words_lookup(num: f64, prev: bool, ord: bool) -> String {
    if num <= 19.0 {
        format!(
            "{}{}",
            if prev { " and " } else { "" },
            if ord {
                ORDINALS[num as usize]
            } else {
                FEW[num as usize]
            }
        )
    } else if num < 100.0 {
        let tens = (num / 10.0).floor();
        let remainder = num - tens * 10.0;
        let mut words = format!(
            "{}{}",
            if prev { " and " } else { "" },
            DECADES[(tens as usize) - 2]
        );
        if remainder > 0.0 {
            words.push('-');
            words.push_str(&words_lookup(remainder, false, ord));
        } else if ord {
            words.truncate(words.len() - 1);
            words.push_str("ieth");
        }
        words
    } else if num < 1000.0 {
        let hundreds = (num / 100.0).floor();
        let remainder = num - hundreds * 100.0;
        let mut words = format!(
            "{}{} Hundred",
            if prev { ", " } else { "" },
            FEW[hundreds as usize]
        );
        if remainder > 0.0 {
            words.push_str(&words_lookup(remainder, true, ord));
        } else if ord {
            words.push_str("th");
        }
        words
    } else {
        let mut mag = (num.log10() / 3.0).floor() as usize;
        if mag > MAGNITUDES.len() {
            mag = MAGNITUDES.len();
        }
        let factor = 10f64.powi((mag * 3) as i32);
        let mant = (num / factor).floor();
        let remainder = num - mant * factor;
        let mut words = format!(
            "{}{} {}",
            if prev { ", " } else { "" },
            words_lookup(mant, false, false),
            MAGNITUDES[mag - 1]
        );
        if remainder > 0.0 {
            words.push_str(&words_lookup(remainder, true, ord));
        } else if ord {
            words.push_str("th");
        }
        words
    }
}

/// Converts a number into English words
fn number_to_words(value: f64, ordinal: bool) -> String {
    if value.is_nan() {
        // A component of an out-of-range date: empty, as in roman numerals
        // and letters (jsonata-js 2.3.0).
        return String::new();
    }
    words_lookup(value, false, ordinal)
}

fn word_values() -> &'static HashMap<String, f64> {
    static MAP: OnceLock<HashMap<String, f64>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = HashMap::new();
        for (i, w) in FEW.iter().enumerate() {
            m.insert(w.to_lowercase(), i as f64);
        }
        for (i, w) in ORDINALS.iter().enumerate() {
            m.insert(w.to_lowercase(), i as f64);
        }
        for (i, w) in DECADES.iter().enumerate() {
            let lword = w.to_lowercase();
            let val = (i as f64 + 2.0) * 10.0;
            let ieth = format!("{}ieth", &lword[..lword.len() - 1]);
            m.insert(lword, val);
            m.insert(ieth, val);
        }
        m.insert("hundredth".to_string(), 100.0);
        for (i, w) in MAGNITUDES.iter().enumerate() {
            let lword = w.to_lowercase();
            let val = 10f64.powi(((i + 1) * 3) as i32);
            m.insert(format!("{lword}th"), val);
            m.insert(lword, val);
        }
        m
    })
}

fn word_split_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r",\s|\sand\s|[\s\-]").unwrap())
}

/// Converts a number in English words to its numeric value
fn words_to_number(text: &str) -> Option<f64> {
    let parts: Vec<&str> = word_split_regex().split(text).collect();
    let values: Option<Vec<f64>> = parts
        .iter()
        .map(|p| word_values().get(*p).copied())
        .collect();
    let values = values?;
    let mut segs: Vec<f64> = vec![0.0];
    for value in values {
        if value < 100.0 {
            let mut top = segs.pop().unwrap();
            if top >= 1000.0 {
                segs.push(top);
                top = 0.0;
            }
            segs.push(top + value);
        } else {
            let top = segs.pop().unwrap();
            segs.push(top * value);
        }
    }
    Some(segs.iter().sum())
}

// ── Roman numerals ────────────────────────────────────────────────────────────

const ROMAN_NUMERALS: [(i64, &str); 13] = [
    (1000, "m"),
    (900, "cm"),
    (500, "d"),
    (400, "cd"),
    (100, "c"),
    (90, "xc"),
    (50, "l"),
    (40, "xl"),
    (10, "x"),
    (9, "ix"),
    (5, "v"),
    (4, "iv"),
    (1, "i"),
];

/// The largest number formatted in roman numerals (jsonata-js 2.3.0
/// `maxRoman`): ten thousand Ms. §4.6.1 allows an upper bound, and larger
/// numbers use the format token `1`.
const MAX_ROMAN: f64 = 1e7;

fn decimal_to_roman(mut value: i64) -> String {
    let mut result = String::new();
    for &(v, s) in ROMAN_NUMERALS.iter() {
        while value >= v {
            result.push_str(s);
            value -= v;
        }
    }
    result
}

fn roman_value(c: char) -> Option<f64> {
    match c {
        'M' => Some(1000.0),
        'D' => Some(500.0),
        'C' => Some(100.0),
        'L' => Some(50.0),
        'X' => Some(10.0),
        'V' => Some(5.0),
        'I' => Some(1.0),
        _ => None,
    }
}

/// NaN for a character that isn't an upper-case numeral, as jsonata-js's
/// `romanToDecimal` gives (it adds undefined).
fn roman_to_decimal(roman: &str) -> f64 {
    let mut decimal = 0.0;
    let mut max = 1.0;
    for c in roman.chars().rev() {
        let Some(value) = roman_value(c) else {
            return f64::NAN;
        };
        if value < max {
            decimal -= value;
        } else {
            max = value;
            decimal += value;
        }
    }
    decimal
}

// ── Spreadsheet-style letters (A, B, ... Z, AA, AB, ...) ─────────────────────

fn decimal_to_letters(mut value: i64, a_char: char) -> String {
    let a_code = a_char as u32;
    let mut letters = Vec::new();
    while value > 0 {
        let rem = ((value - 1) % 26) as u32;
        letters.push(char::from_u32(a_code + rem).unwrap());
        value = (value - 1) / 26;
    }
    letters.iter().rev().collect()
}

/// In JavaScript numbers, which a long run of letters can't overflow.
fn letters_to_decimal(letters: &str, a_char: char) -> f64 {
    let a_code = a_char as u32 as f64;
    let mut decimal = 0.0;
    for (i, c) in letters.chars().rev().enumerate() {
        decimal += (c as u32 as f64 - a_code + 1.0) * 26f64.powi(i as i32);
    }
    decimal
}

// ── fn:format-integer picture-string analysis ────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TCase {
    Upper,
    Lower,
    Title,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrimaryFormat {
    Decimal,
    Letters,
    Roman,
    Words,
    Sequence,
}

#[derive(Debug, Clone)]
struct GroupingSeparator {
    position: usize,
    character: char,
}

#[derive(Debug, Clone)]
struct IntegerFormat {
    primary: PrimaryFormat,
    case: TCase,
    ordinal: bool,
    zero_code: u32,
    mandatory_digits: usize,
    optional_digits: usize,
    regular: bool,
    grouping_separators: Vec<GroupingSeparator>,
    /// Set when this marker is immediately followed by another integer-formatted
    /// marker with no literal in between -- fixes the parse width so the regex
    /// doesn't greedily consume digits belonging to the next component.
    parse_width: Option<i64>,
    /// A year whose width modifier has a negative maximum (`[Y,1--2]`):
    /// jsonata-js makes its mandatory digits negative, a parse width that
    /// matches nothing.
    negative_width: bool,
    /// A year whose width modifier's maximum isn't a number (`[Y,2-x]`):
    /// NaN digits in jsonata-js, so no parse width.
    nan_width: bool,
    /// Only set for `PrimaryFormat::Sequence` (unsupported "numbering sequence"),
    /// carried for the D3130 error message.
    token: Option<String>,
}

impl Default for IntegerFormat {
    fn default() -> Self {
        IntegerFormat {
            primary: PrimaryFormat::Decimal,
            case: TCase::Lower,
            ordinal: false,
            zero_code: 0x30,
            mandatory_digits: 0,
            optional_digits: 0,
            regular: false,
            grouping_separators: Vec::new(),
            parse_width: None,
            negative_width: false,
            nan_width: false,
            token: None,
        }
    }
}

// TODO: what about decimal digit groups in the unicode supplementary planes (surrogate
// pairs)? -- matches a pre-existing "TODO" in the JS reference implementation.
const DECIMAL_GROUPS: [u32; 37] = [
    0x30, 0x0660, 0x06F0, 0x07C0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0B66, 0x0BE6, 0x0C66, 0x0CE6,
    0x0D66, 0x0DE6, 0x0E50, 0x0ED0, 0x0F20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80,
    0x1A90, 0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0,
    0xFF10,
];

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// Are the grouping-separator-signs "regular" (same character, equally spaced)?
/// Returns the common spacing factor, or 0 if irregular.
fn regular_repeat(separators: &[GroupingSeparator]) -> usize {
    if separators.is_empty() {
        return 0;
    }
    let sep_char = separators[0].character;
    if separators.iter().any(|s| s.character != sep_char) {
        return 0;
    }
    let indexes: Vec<usize> = separators.iter().map(|s| s.position).collect();
    let factor = indexes.iter().skip(1).fold(indexes[0], |a, &b| gcd(a, b));
    if factor == 0 {
        return 0;
    }
    // A set, as in jsonata-js 2.3.0, so a long picture is linear (#869).
    let positions: std::collections::HashSet<usize> = indexes.iter().copied().collect();
    for index in 1..=indexes.len() {
        if !positions.contains(&(index * factor)) {
            return 0;
        }
    }
    factor
}

/// Analyse a `fn:format-integer` picture string (e.g. "0001", "I", "w", "9,999").
fn analyse_integer_picture(picture: &str) -> Result<IntegerFormat, DateTimeError> {
    let mut format = IntegerFormat::default();

    let semicolon = picture.rfind(';');
    let primary_format = match semicolon {
        Some(idx) => &picture[..idx],
        None => picture,
    };
    if let Some(idx) = semicolon {
        let format_modifier = &picture[idx + 1..];
        if format_modifier.starts_with('o') {
            format.ordinal = true;
        }
    }

    match primary_format {
        "A" => {
            format.case = TCase::Upper;
            format.primary = PrimaryFormat::Letters;
        }
        "a" => {
            format.primary = PrimaryFormat::Letters;
        }
        "I" => {
            format.case = TCase::Upper;
            format.primary = PrimaryFormat::Roman;
        }
        "i" => {
            format.primary = PrimaryFormat::Roman;
        }
        "W" => {
            format.case = TCase::Upper;
            format.primary = PrimaryFormat::Words;
        }
        "Ww" => {
            format.case = TCase::Title;
            format.primary = PrimaryFormat::Words;
        }
        "w" => {
            format.primary = PrimaryFormat::Words;
        }
        _ => {
            let mut zero_code: Option<u32> = None;
            let mut mandatory_digits = 0usize;
            let mut optional_digits = 0usize;
            let mut grouping_separators: Vec<GroupingSeparator> = Vec::new();
            let mut separator_position = 0usize;

            // Reverse the codepoints to determine positions of grouping-separator-signs
            // from the least-significant digit.
            let codepoints: Vec<u32> = primary_format.chars().rev().map(|c| c as u32).collect();
            for cp in codepoints {
                let mut is_digit = false;
                for &group in DECIMAL_GROUPS.iter() {
                    if cp >= group && cp <= group + 9 {
                        is_digit = true;
                        mandatory_digits += 1;
                        separator_position += 1;
                        match zero_code {
                            None => zero_code = Some(group),
                            Some(z) if z != group => {
                                return Err(coded(
                                    "D3131",
                                    "mixed decimal-digit families in picture",
                                ));
                            }
                            _ => {}
                        }
                        break;
                    }
                }
                if !is_digit {
                    if cp == 0x23 {
                        // '#' - optional-digit-sign
                        separator_position += 1;
                        optional_digits += 1;
                    } else {
                        grouping_separators.push(GroupingSeparator {
                            position: separator_position,
                            character: char::from_u32(cp).unwrap_or('?'),
                        });
                    }
                }
            }

            if mandatory_digits > 0 {
                format.primary = PrimaryFormat::Decimal;
                format.zero_code = zero_code.unwrap();
                format.mandatory_digits = mandatory_digits;
                format.optional_digits = optional_digits;

                let regular = regular_repeat(&grouping_separators);
                if regular > 0 {
                    format.regular = true;
                    format.grouping_separators = vec![GroupingSeparator {
                        position: regular,
                        character: grouping_separators[0].character,
                    }];
                } else {
                    format.regular = false;
                    format.grouping_separators = grouping_separators;
                }
            } else {
                // A "numbering sequence" -- implementation-defined by the spec, and not
                // supported by this implementation (matches the JS reference).
                format.primary = PrimaryFormat::Sequence;
                format.token = Some(primary_format.to_string());
            }
        }
    }

    Ok(format)
}

/// Formats an integer as specified by a preprocessed `IntegerFormat` (mirrors
/// datetime.js's private `_formatInteger`).
///
/// Takes `f64` (matching JS's Number type) rather than `i64` because the `words` format
/// must support magnitudes far beyond `i64::MAX` (e.g. 1e46). Decimal digits, and roman
/// numerals past their limit, are written from the number's shortest decimal digits, so
/// they are exact at any magnitude. Letters and smaller roman numerals are cast down to
/// `i64` internally -- safe for the magnitudes they're actually specified and tested for.
fn format_integer_value(value: f64, format: &IntegerFormat) -> Result<String, DateTimeError> {
    let negative = value < 0.0;
    let value = value.abs();

    let mut formatted = match format.primary {
        PrimaryFormat::Letters => decimal_to_letters(
            value as i64,
            if format.case == TCase::Upper {
                'A'
            } else {
                'a'
            },
        ),
        // There is no numeral larger than M, so a large number would be a
        // long run of them (1e15 is 1e12 Ms). Like jsonata-js 2.3.0, numbers
        // above ten thousand Ms are formatted as plain decimal digits, from
        // the shortest round-trip digits like the Decimal arm (#864), and an
        // infinite number as `Infinity`.
        PrimaryFormat::Roman if value > MAX_ROMAN => {
            crate::functions::decimal::decimal_digits_to_string(
                &crate::functions::decimal::decimal_digits(value.trunc()),
                0,
            )
        }
        PrimaryFormat::Roman => {
            let r = decimal_to_roman(value as i64);
            if format.case == TCase::Upper {
                r.to_uppercase()
            } else {
                r
            }
        }
        PrimaryFormat::Words => {
            let w = number_to_words(value, format.ordinal);
            match format.case {
                TCase::Upper => w.to_uppercase(),
                TCase::Lower => w.to_lowercase(),
                TCase::Title => w,
            }
        }
        PrimaryFormat::Decimal => {
            // The shortest round-trip digits written out in full, as
            // jsonata-js 2.3.0 does (#864, #868): exact above i64, where a
            // cast would saturate. An infinite number is `Infinity`, which
            // padding and grouping then treat as digits, as jsonata-js does.
            let mut digits = crate::functions::decimal::decimal_digits_to_string(
                &crate::functions::decimal::decimal_digits(value),
                0,
            );
            if format.mandatory_digits > digits.len() {
                digits = format!(
                    "{}{}",
                    "0".repeat(format.mandatory_digits - digits.len()),
                    digits
                );
            }
            if format.zero_code != 0x30 {
                digits = digits
                    .chars()
                    // Every character moves by the family's offset, as in
                    // jsonata-js: the "digits" of NaN and Infinity too.
                    .map(|c| char::from_u32(c as u32 + format.zero_code - 0x30).unwrap_or(c))
                    .collect();
            }
            // Insert the grouping-separator-signs, if any.
            let mut chars: Vec<char> = digits.chars().collect();
            if format.regular {
                let sep = &format.grouping_separators[0];
                let n = chars.len().saturating_sub(1) / sep.position;
                for ii in (1..=n).rev() {
                    let pos = chars.len() - ii * sep.position;
                    chars.insert(pos, sep.character);
                }
            } else {
                let mut seps = format.grouping_separators.clone();
                seps.reverse();
                for sep in seps {
                    let pos = chars.len().saturating_sub(sep.position);
                    chars.insert(pos, sep.character);
                }
            }
            chars.into_iter().collect()
        }
        PrimaryFormat::Sequence => {
            return Err(coded("D3130", format.token.clone().unwrap_or_default()));
        }
    };

    if format.primary == PrimaryFormat::Decimal && format.ordinal {
        let chars: Vec<char> = formatted.chars().collect();
        let last = *chars.last().unwrap();
        let second_last = if chars.len() >= 2 {
            Some(chars[chars.len() - 2])
        } else {
            None
        };
        let mut suffix = match last {
            '1' => "st",
            '2' => "nd",
            '3' => "rd",
            _ => "th",
        };
        if second_last == Some('1') {
            // 11th, 12th, 13th (and their thousands multiples)
            suffix = "th";
        }
        formatted.push_str(suffix);
    }

    if negative {
        formatted = format!("-{formatted}");
    }

    Ok(formatted)
}

// ═══════════════════════════════════════════════════════════════════════════
// §9.8.4 fn:format-dateTime / fn:parse-dateTime picture-string analysis
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
struct WidthSpec {
    min: Option<usize>,
    max: Option<usize>,
    /// The maximum was a negative number, which `max` leaves out.
    max_negative: bool,
    /// The minimum or maximum was text `parseInt` can't read, which
    /// jsonata-js keeps as NaN (`min`/`max` leave it out).
    min_nan: bool,
    max_nan: bool,
}

#[derive(Debug, Clone)]
struct MarkerSpec {
    component: char,
    width: Option<WidthSpec>,
    presentation1: String,
    presentation2: Option<char>,
    names: Option<TCase>,
    integer_format: Option<IntegerFormat>,
    /// Year-truncation digit count (§9.8.4.4); `None` means no truncation.
    n: Option<u32>,
}

#[derive(Debug, Clone)]
enum PictureItem {
    Literal(String),
    Marker(MarkerSpec),
}

fn default_presentation_modifier(component: char) -> Option<&'static str> {
    Some(match component {
        'Y' => "1",
        'M' => "1",
        'D' => "1",
        'd' => "1",
        'F' => "n",
        'W' => "1",
        'w' => "1",
        'X' => "1",
        'x' => "1",
        'H' => "1",
        'h' => "1",
        'P' => "n",
        'm' => "01",
        's' => "01",
        'f' => "1",
        'Z' => "01:01",
        'z' => "01:01",
        'C' => "n",
        'E' => "n",
        _ => return None,
    })
}

fn add_literal(spec: &mut Vec<PictureItem>, lit: &str) {
    if !lit.is_empty() {
        let literal = lit.replace("]]", "]");
        spec.push(PictureItem::Literal(literal));
    }
}

/// The largest width modifier allowed in a date/time picture string
/// (jsonata-js 2.3.0 `maxWidthModifier`, error D3142).
const MAX_WIDTH_MODIFIER: usize = 100;

/// Analyse a date/time picture string (§9.8.4.1).
fn analyse_date_time_picture(picture: &str) -> Result<Vec<PictureItem>, DateTimeError> {
    let chars: Vec<char> = picture.chars().collect();
    let n = chars.len();
    let mut spec: Vec<PictureItem> = Vec::new();

    let mut start = 0usize;
    let mut pos = 0usize;
    while pos < n {
        if chars[pos] == '[' {
            if pos + 1 < n && chars[pos + 1] == '[' {
                add_literal(&mut spec, &chars[start..pos].iter().collect::<String>());
                spec.push(PictureItem::Literal("[".to_string()));
                pos += 2;
                start = pos;
                continue;
            }

            add_literal(&mut spec, &chars[start..pos].iter().collect::<String>());
            start = pos;

            let close = match chars[pos..].iter().position(|&c| c == ']') {
                Some(i) => pos + i,
                None => {
                    return Err(coded("D3135", "no closing bracket in date/time picture"));
                }
            };

            let raw_marker: String = chars[start + 1..close].iter().collect();
            let marker: String = raw_marker.split_whitespace().collect();
            if marker.is_empty() {
                return Err(coded("D3135", "empty component specifier"));
            }
            let component = marker.chars().next().unwrap();

            let comma = marker.rfind(',');
            let (pres_mod, width) = if let Some(comma_idx) = comma {
                let width_mod = &marker[comma_idx + 1..];
                let dash = width_mod.find('-');
                let parse_width = |s: &str| -> Result<Option<f64>, DateTimeError> {
                    if s.is_empty() || s == "*" {
                        Ok(None)
                    } else {
                        // jsonata-js 2.3.0 rejects a width modifier above 100
                        // (D3142): a component is padded to its minimum width,
                        // so a huge width would build a huge string (or, for
                        // `Y`, overflow `10^n`). The test uses JS `parseInt`,
                        // so a value too large for usize is still caught.
                        if js_parse_int(s) > MAX_WIDTH_MODIFIER as f64 {
                            return Err(coded(
                                "D3142",
                                format!(
                                    "A width modifier in a date/time picture string cannot be greater than 100: \"{}\"",
                                    s
                                ),
                            ));
                        }
                        // JavaScript's `parseInt`: `[Y,5x]` is a width of 5
                        // (#302 item 12). NaN, and a negative width, are
                        // taken as no width.
                        Ok(Some(js_parse_int(s)))
                    }
                };
                let (min, max) = match dash {
                    Some(d) => (
                        parse_width(&width_mod[..d])?,
                        parse_width(&width_mod[d + 1..])?,
                    ),
                    None => (parse_width(width_mod)?, None),
                };
                (
                    marker[1..comma_idx].to_string(),
                    Some(WidthSpec {
                        min: min.filter(|w| *w >= 0.0).map(|w| w as usize),
                        max: max.filter(|w| *w >= 0.0).map(|w| w as usize),
                        max_negative: max.is_some_and(|w| w < 0.0),
                        min_nan: min.is_some_and(f64::is_nan),
                        max_nan: max.is_some_and(f64::is_nan),
                    }),
                )
            } else {
                (marker[1..].to_string(), None)
            };

            let mut m = MarkerSpec {
                component,
                width,
                presentation1: String::new(),
                presentation2: None,
                names: None,
                integer_format: None,
                n: None,
            };

            let pres_chars: Vec<char> = pres_mod.chars().collect();
            if pres_chars.len() == 1 {
                m.presentation1 = pres_mod;
            } else if pres_chars.len() > 1 {
                let last_char = *pres_chars.last().unwrap();
                if "atco".contains(last_char) {
                    m.presentation2 = Some(last_char);
                    m.presentation1 = pres_chars[..pres_chars.len() - 1].iter().collect();
                } else {
                    m.presentation1 = pres_mod;
                }
            } else {
                m.presentation1 = default_presentation_modifier(component)
                    .ok_or_else(|| coded("D3132", component.to_string()))?
                    .to_string();
            }

            let p1_chars: Vec<char> = m.presentation1.chars().collect();
            if p1_chars[0] == 'n' {
                m.names = Some(TCase::Lower);
            } else if p1_chars[0] == 'N' {
                if p1_chars.get(1) == Some(&'n') {
                    m.names = Some(TCase::Title);
                } else {
                    m.names = Some(TCase::Upper);
                }
            } else if "YMDdFWwXxHhmsf".contains(component) {
                let mut integer_pattern = m.presentation1.clone();
                if let Some(p2) = m.presentation2 {
                    integer_pattern.push(';');
                    integer_pattern.push(p2);
                }
                let mut integer_format = analyse_integer_picture(&integer_pattern)?;
                if let Some(w) = &m.width {
                    if let Some(min) = w.min {
                        if integer_format.mandatory_digits < min {
                            integer_format.mandatory_digits = min;
                        }
                    }
                }
                if component == 'Y' {
                    // §9.8.4.4 Formatting/parsing the Year Component
                    let mut n_val: Option<u32> = None;
                    if m.width.as_ref().is_some_and(|w| w.max_negative) {
                        integer_format.negative_width = true;
                    }
                    if m.width.as_ref().is_some_and(|w| w.max_nan) {
                        // `n` and the mandatory digits are NaN in jsonata-js:
                        // the year formats as NaN, and parses with no width.
                        integer_format.nan_width = true;
                    }
                    if let Some(w) = &m.width {
                        if let Some(max) = w.max {
                            n_val = Some(max as u32);
                            integer_format.mandatory_digits = max;
                        }
                    }
                    if n_val.is_none() {
                        let w = integer_format.mandatory_digits + integer_format.optional_digits;
                        if w >= 2 {
                            n_val = Some(w as u32);
                        }
                    }
                    m.n = n_val;
                }
                // If the previous part is also an integer with no intervening markup,
                // its parse width must be precisely defined (so parsing doesn't
                // greedily consume digits belonging to this marker) -- unless this
                // part can't start with a digit, so there's no doubt where the
                // previous one ends. Fractional seconds are always parsed as
                // digits, whatever their presentation (jsonata-js 2.3.0).
                if integer_format.primary == PrimaryFormat::Decimal || component == 'f' {
                    if let Some(PictureItem::Marker(prev)) = spec.last_mut() {
                        if let Some(prev_fmt) = &mut prev.integer_format {
                            prev_fmt.parse_width = if prev_fmt.nan_width {
                                // A NaN width is no width (it is falsy).
                                None
                            } else if prev_fmt.negative_width {
                                Some(-1)
                            } else {
                                Some(prev_fmt.mandatory_digits as i64)
                            };
                        }
                    }
                }
                m.integer_format = Some(integer_format);
            }

            if component == 'Z' || component == 'z' {
                m.integer_format = Some(analyse_integer_picture(&m.presentation1)?);
            }

            spec.push(PictureItem::Marker(m));
            start = close + 1;
            pos = close + 1;
            continue;
        }
        pos += 1;
    }
    add_literal(&mut spec, &chars[start..pos].iter().collect::<String>());

    Ok(spec)
}

// ── Date/time component extraction & week-of-year/month arithmetic ──────────

const DAYS: [&str; 8] = [
    "",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn next_month(year: i64, month0: i64) -> (i64, i64) {
    if month0 == 11 {
        (year + 1, 0)
    } else {
        (year, month0 + 1)
    }
}

fn prev_month(year: i64, month0: i64) -> (i64, i64) {
    if month0 == 0 {
        (year - 1, 11)
    } else {
        (year, month0 - 1)
    }
}

/// `Date.UTC` reads a year from 0 to 99 as 1900 to 1999.
fn js_utc_year(year: i64) -> i64 {
    if (0..=99).contains(&year) {
        year + 1900
    } else {
        year
    }
}

/// The date `Date.UTC(year, month0, day)` names in jsonata-js. The week and
/// day-of-year calculations go through it, so they inherit its reading of the
/// years 0-99, as there. The year is one of a real date, so nothing here can
/// overflow.
fn js_utc_date(year: i64, month0: i64, day: i64) -> CivilDate {
    let year = js_utc_year(year) + month0.div_euclid(12);
    CivilDate {
        days: days_from_civil(year, month0.rem_euclid(12) + 1, 1) + day - 1,
    }
}

/// ISO 8601 defines the first week of the year (or, by extension, of the month) to be
/// the week containing the first Thursday. Weeks start on Monday.
fn start_of_first_week(year: i64, month0: i64) -> CivilDate {
    let first = js_utc_date(year, month0, 1);
    let day_of_first = first.weekday_from_monday(); // Mon=1 .. Sun=7
    if day_of_first > 4 {
        first.plus_days(8 - day_of_first)
    } else {
        first.plus_days(-(day_of_first - 1))
    }
}

fn delta_weeks(start: CivilDate, end: CivilDate) -> f64 {
    (end.days - start.days) as f64 / 7.0 + 1.0
}

fn week_in_year(date: CivilDate) -> i64 {
    let (year, month, day) = date.ymd();
    let date = js_utc_date(year, month - 1, day);
    let start_of_week1 = start_of_first_week(year, 0);
    let mut week = delta_weeks(start_of_week1, date);
    if week > 52.0 {
        let start_of_following_year = start_of_first_week(year + 1, 0);
        if date >= start_of_following_year {
            week = 1.0;
        }
    } else if week < 1.0 {
        let start_of_previous_year = start_of_first_week(year - 1, 0);
        week = delta_weeks(start_of_previous_year, date);
    }
    week.floor() as i64
}

fn week_in_month(date: CivilDate) -> i64 {
    let (year, month, day) = date.ymd();
    let month0 = month - 1;
    let date = js_utc_date(year, month0, day);
    let start_of_week1 = start_of_first_week(year, month0);
    let mut week = delta_weeks(start_of_week1, date);
    if week > 4.0 {
        let (ny, nm) = next_month(year, month0);
        let start_of_following_month = start_of_first_week(ny, nm);
        if date >= start_of_following_month {
            week = 1.0;
        }
    } else if week < 1.0 {
        let (py, pm) = prev_month(year, month0);
        let start_of_previous_month = start_of_first_week(py, pm);
        week = delta_weeks(start_of_previous_month, date);
    }
    week.floor() as i64
}

/// Extension: ISO week-numbering year -- the year associated with the ISO week (e.g.
/// Sat 1 Jan 2005 is in the 53rd week of 2004).
fn iso_week_year(date: CivilDate) -> i64 {
    let year = date.year();
    let start_of_iso_year = start_of_first_week(year, 0);
    let end_of_iso_year = start_of_first_week(year + 1, 0);
    if date < start_of_iso_year {
        (year - 1) as i64
    } else if date >= end_of_iso_year {
        (year + 1) as i64
    } else {
        year as i64
    }
}

/// Extension: the "week-numbering" month, i.e. the month associated with the
/// week-of-the-month (e.g. Sat 1 Jan 2005 is in the 5th week of December 2004).
/// Returns a 1-indexed month number.
fn iso_week_month(date: CivilDate) -> i64 {
    let year = date.year();
    let month0 = date.month0();
    let start_of_iso_month = start_of_first_week(year, month0);
    let (ny, nm) = next_month(year, month0);
    let end_of_iso_month = start_of_first_week(ny, nm);
    if date < start_of_iso_month {
        let (_, pm) = prev_month(year, month0);
        (pm + 1) as i64
    } else if date >= end_of_iso_month {
        (nm + 1) as i64
    } else {
        (month0 + 1) as i64
    }
}

/// Extracts a single date/time component's raw numeric value (day/week/hour/etc). `P`
/// (am/pm) is returned as 0/1; `C`/`E` (calendar/era, always "ISO") and `Z`/`z`
/// (timezone, formatted separately from the offset) are not meaningful here and return 0.
fn get_date_time_fragment(millis: i64, component: char) -> i64 {
    let date = CivilDate::from_millis(millis);
    let time_of_day = millis.rem_euclid(MILLIS_PER_DAY);
    let hour = time_of_day / 3_600_000;
    match component {
        'Y' => date.year(),
        'M' => date.month0() + 1,
        'D' => date.ymd().2,
        'd' => {
            // As jsonata-js: the days from `Date.UTC(year, 0)` to
            // `Date.UTC(year, month, day)`, which for a year 0-99 counts in
            // the 1900s' calendar.
            let (year, month, day) = date.ymd();
            js_utc_date(year, month - 1, day).days - js_utc_date(year, 0, 1).days + 1
        }
        'F' => date.weekday_from_monday(),
        'W' => week_in_year(date),
        'w' => week_in_month(date),
        'X' => iso_week_year(date),
        'x' => iso_week_month(date),
        'H' => hour,
        'h' => {
            let h = hour % 12;
            if h == 0 {
                12
            } else {
                h
            }
        }
        'P' => {
            if hour >= 12 {
                1
            } else {
                0
            }
        }
        'm' => time_of_day / 60_000 % 60,
        's' => time_of_day / 1000 % 60,
        'f' => time_of_day % 1000,
        _ => 0,
    }
}

// ── §9.8.4.3-9.8.4.7 Formatting ───────────────────────────────────────────────

fn two_digit_decimal_format() -> IntegerFormat {
    // A minimal, always-valid "00" pattern (2 mandatory digits, no separators, no
    // ordinal), used for the "hh:mm" fallback in irregular timezone formatting.
    analyse_integer_picture("00").expect("static pattern is always valid")
}

/// `offset_hours` and `offset_minutes` are NaN for a timezone that isn't a
/// number, as in jsonata-js: the sign is then `+`, the minutes are always
/// written, and `t` never makes it `Z`.
fn format_timezone_component(
    offset_hours: f64,
    offset_minutes: f64,
    marker: &MarkerSpec,
) -> Result<String, DateTimeError> {
    let offset = offset_hours * 100.0 + offset_minutes;
    let fmt = marker
        .integer_format
        .as_ref()
        .expect("Z/z marker always has an integer_format");

    // The sign is written once, before both the hours and the minutes, so
    // the parts are formatted as magnitudes: -03:30, not -3:-30 (jsonata-js
    // 2.3.0, #856).
    let mut component_value = if fmt.regular {
        format_integer_value(offset.abs(), fmt)?
    } else {
        let num_digits = fmt.mandatory_digits;
        if num_digits == 1 || num_digits == 2 {
            let mut cv = format_integer_value(offset_hours.abs(), fmt)?;
            // `NaN !== 0` in jsonata-js, so NaN minutes are written too.
            if offset_minutes != 0.0 {
                cv.push(':');
                cv.push_str(&format_integer_value(
                    offset_minutes.abs(),
                    &two_digit_decimal_format(),
                )?);
            }
            cv
        } else if num_digits == 3 || num_digits == 4 {
            format_integer_value(offset.abs(), fmt)?
        } else {
            return Err(coded("D3134", num_digits.to_string()));
        }
    };

    component_value = format!("{}{component_value}", if offset < 0.0 { '-' } else { '+' });
    if marker.component == 'z' {
        component_value = format!("GMT{component_value}");
    }
    if offset == 0.0 && marker.presentation2 == Some('t') {
        component_value = "Z".to_string();
    }

    Ok(component_value)
}

/// `local_millis` is `None` for a date outside JavaScript's range, whose
/// components are all NaN in jsonata-js: digits format as `NaN`, and names,
/// words, roman numerals and letters as empty strings.
fn format_component(
    local_millis: Option<i64>,
    offset_hours: f64,
    offset_minutes: f64,
    marker: &MarkerSpec,
) -> Result<String, DateTimeError> {
    if marker.component == 'Z' || marker.component == 'z' {
        return format_timezone_component(offset_hours, offset_minutes, marker);
    }

    if marker.component == 'P' {
        // §9.8.4.7 am/pm marker
        // NaN hours compare false with 12, so an out-of-range date is "am".
        let mut s = if local_millis.is_some_and(|ms| get_date_time_fragment(ms, 'P') == 1) {
            "pm"
        } else {
            "am"
        }
        .to_string();
        if marker.names == Some(TCase::Upper) {
            s = s.to_uppercase();
        }
        return Ok(s);
    }

    if "YMDdFWwXxHhms".contains(marker.component) {
        // §9.8.4.3 Formatting Integer-Valued Date/Time Components
        let Some(local_millis) = local_millis else {
            if marker.names.is_some() {
                if !"MxF".contains(marker.component) {
                    return Err(coded("D3133", marker.component.to_string()));
                }
                return Ok(String::new());
            }
            let fmt = marker
                .integer_format
                .as_ref()
                .expect("numeric marker always has an integer_format");
            return format_integer_value(f64::NAN, fmt);
        };
        let mut value = get_date_time_fragment(local_millis, marker.component);
        if marker.component == 'Y' && marker.width.as_ref().is_some_and(|w| w.max_nan) {
            // `value % Math.pow(10, NaN)` in jsonata-js.
            let fmt = marker
                .integer_format
                .as_ref()
                .expect("numeric marker always has an integer_format");
            return format_integer_value(f64::NAN, fmt);
        }
        if marker.component == 'Y' {
            if let Some(n) = marker.n {
                // `10^n` overflows i64 from n = 19, when every i64 year is
                // already below it, so the modulus is then a no-op (as
                // `value % Math.pow(10, n)` is in JS).
                if let Some(modulus) = 10i64.checked_pow(n) {
                    value %= modulus;
                }
            }
        }
        if let Some(case) = marker.names {
            let name = match marker.component {
                'M' | 'x' => MONTHS[(value - 1) as usize].to_string(),
                'F' => DAYS[value as usize].to_string(),
                other => return Err(coded("D3133", other.to_string())),
            };
            let mut name = match case {
                TCase::Upper => name.to_uppercase(),
                TCase::Lower => name.to_lowercase(),
                TCase::Title => name,
            };
            if let Some(w) = &marker.width {
                if let Some(max) = w.max {
                    if name.chars().count() > max {
                        name = name.chars().take(max).collect();
                    }
                }
                if w.max_negative {
                    // `name.substring(0, -2)` in jsonata-js.
                    name.clear();
                }
            }
            Ok(name)
        } else {
            let fmt = marker
                .integer_format
                .as_ref()
                .expect("numeric marker always has an integer_format");
            format_integer_value(value as f64, fmt)
        }
    } else if marker.component == 'f' {
        // §9.8.4.5 Formatting Fractional Seconds
        // A name presentation (`[fN]`) leaves no integer format: fractional
        // seconds can't be formatted as a name (D3133, as in jsonata-js
        // 2.3.0) -- this used to panic.
        let fmt = marker
            .integer_format
            .as_ref()
            .ok_or_else(|| coded("D3133", "f"))?;
        let Some(local_millis) = local_millis else {
            return format_integer_value(f64::NAN, fmt);
        };
        let value = get_date_time_fragment(local_millis, 'f');
        if fmt.primary != PrimaryFormat::Decimal {
            return format_integer_value(value as f64, fmt);
        }
        format_fractional_seconds(value, marker)
    } else {
        // C (calendar) / E (era) -- always "ISO"
        Ok("ISO".to_string())
    }
}

/// The digit range of a fractional-seconds marker (jsonata-js 2.3.0
/// `fractionalSecondsFormat`, #855): the picture's own digits, widened by a
/// width modifier, and up to the full millisecond precision for a picture
/// of one digit with no width modifier.
///
/// `None` for the digit range when a width modifier isn't a number: the
/// range is NaN in jsonata-js, which formats no digits and fixes no width.
#[allow(clippy::type_complexity)]
fn fractional_seconds_format(
    marker: &MarkerSpec,
) -> Result<(Vec<char>, IntegerFormat, Option<(usize, usize)>), DateTimeError> {
    let picture: Vec<char> = marker.presentation1.chars().collect();
    // An empty modifier keeps semicolons in the primary token grouping signs.
    let format = analyse_integer_picture(&format!("{};", marker.presentation1))?;
    let mut min_digits = format.mandatory_digits;
    let mut max_digits = min_digits + format.optional_digits;
    if let Some(width) = &marker.width {
        if width.min_nan || width.max_nan {
            return Ok((picture, format, None));
        }
        if let Some(min) = width.min {
            min_digits = min_digits.max(min);
        }
        max_digits = max_digits.max(min_digits);
        if let Some(max) = width.max {
            max_digits = max_digits.max(max);
        }
    } else if picture.len() == 1 {
        max_digits = 3;
    }
    Ok((picture, format, Some((min_digits, max_digits))))
}

/// §9.8.4.5: fractional seconds are the digits of a decimal fraction, so
/// they are padded and truncated on the right, never rounded into the next
/// second: 30 ms with `[f01]` is `03` (jsonata-js 2.3.0, #855).
fn format_fractional_seconds(millis: i64, marker: &MarkerSpec) -> Result<String, DateTimeError> {
    let (picture, format, range) = fractional_seconds_format(marker)?;
    let Some((min_digits, max_digits)) = range else {
        return Ok(String::new());
    };
    let mut fraction = format!("{:03}", millis.rem_euclid(1000));
    while fraction.ends_with('0') {
        fraction.pop();
    }
    while fraction.len() < min_digits {
        fraction.push('0');
    }
    let mut digits: Vec<String> = fraction
        .chars()
        .take(max_digits)
        .map(|d| {
            let code = format.zero_code + d.to_digit(10).unwrap_or(0);
            char::from_u32(code).unwrap_or(d).to_string()
        })
        .collect();
    // Grouping positions count from the left. The output never exceeds the
    // adjusted picture's width, so separators never need extrapolating.
    let mut position = 0;
    for character in picture {
        let code = character as u32;
        if character == '#' || (code >= format.zero_code && code <= format.zero_code + 9) {
            position += 1;
        } else if position < digits.len() {
            digits[position] = format!("{character}{}", digits[position]);
        }
    }
    Ok(digits.concat())
}

fn iso_default_picture_spec() -> &'static Vec<PictureItem> {
    static SPEC: OnceLock<Vec<PictureItem>> = OnceLock::new();
    SPEC.get_or_init(|| {
        analyse_date_time_picture("[Y0001]-[M01]-[D01]T[H01]:[m01]:[s01].[f001][Z01:01t]")
            .expect("static ISO picture is always valid")
    })
}

fn parse_leading_int(s: &str) -> Option<i64> {
    let s = s.trim();
    let bytes = s.as_bytes();
    let mut idx = 0;
    if idx < bytes.len() && (bytes[idx] == b'+' || bytes[idx] == b'-') {
        idx += 1;
    }
    let digits_start = idx;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx == digits_start {
        return None;
    }
    s[..idx].parse::<i64>().ok()
}

/// The hours and minutes of a `+hhmm` offset, or `None` for a timezone that
/// isn't a number, which jsonata-js reads as NaN.
fn parse_timezone_offset(timezone: Option<&str>) -> Option<(i64, i64)> {
    // Both parts take the offset's sign: -0330 is -3 hours and -30 minutes
    // (jsonata-js 2.3.0, #856). Flooring the hours read it as -4 and -30.
    let offset = match timezone {
        Some(tz) => parse_leading_int(tz)?,
        None => 0,
    };
    let offset_minutes = offset % 100;
    let offset_hours = (offset - offset_minutes) / 100;
    Some((offset_hours, offset_minutes))
}

/// Formats the date/time as specified by the XPath fn:format-dateTime function.
fn format_date_time(
    millis: f64,
    picture: Option<&str>,
    timezone: Option<&str>,
) -> Result<String, DateTimeError> {
    let parsed_offset = parse_timezone_offset(timezone);
    let (offset_hours, offset_minutes) = parsed_offset.unwrap_or((0, 0));
    // jsonata-js's NaN offset, for the timezone markers. A number too large
    // for i64 is NaN here too, where jsonata-js would still format it.
    let (tz_hours, tz_minutes) = match parsed_offset {
        Some((h, m)) => (h as f64, m as f64),
        None => (f64::NAN, f64::NAN),
    };

    let owned_spec;
    let spec: &Vec<PictureItem> = match picture {
        Some(p) => {
            owned_spec = analyse_date_time_picture(p)?;
            &owned_spec
        }
        None => iso_default_picture_spec(),
    };

    // The time in the target timezone, if it is a JavaScript date: jsonata-js
    // formats `new Date(millis + offsetMillis)`, which drops any fraction of
    // a millisecond (toward zero) after the offset is added. A timestamp or
    // offset outside the range of a JavaScript date, or a timezone that isn't
    // a number, gives an invalid date, whose components jsonata-js 2.3.0
    // formats as NaN (#869).
    let local_millis = parsed_offset
        .and_then(|_| offset_hours.checked_mul(60))
        .and_then(|minutes| minutes.checked_add(offset_minutes))
        .and_then(|minutes| minutes.checked_mul(60 * 1000))
        .map(|offset_millis| (millis + offset_millis as f64).trunc())
        .filter(|shifted| shifted.abs() <= MAX_DATE_MILLIS as f64)
        .map(|shifted| shifted as i64);

    let mut result = String::new();
    for item in spec {
        match item {
            PictureItem::Literal(s) => result.push_str(s),
            PictureItem::Marker(m) => {
                result.push_str(&format_component(local_millis, tz_hours, tz_minutes, m)?);
            }
        }
    }
    Ok(result)
}

// ── §9.8.4 Parsing ────────────────────────────────────────────────────────────

/// How the text a part matched becomes a component value (the `parse`
/// functions of jsonata-js's `generateMatcher`).
#[derive(Debug, Clone)]
enum ParseKind {
    Timezone {
        has_gmt: bool,
        separator: Option<String>,
    },
    Fraction,
    Integer(IntegerFormat),
    Name {
        component: char,
        width_max: Option<usize>,
        /// A negative maximum width makes every name '' in jsonata-js.
        width_negative: bool,
    },
}

/// The character sets a run of characters is matched from, in upper case,
/// since timestamps are matched case-insensitively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharSet {
    Digits,
    Letters,
    Roman,
}

impl CharSet {
    const ALL: [CharSet; 3] = [CharSet::Digits, CharSet::Letters, CharSet::Roman];

    fn index(self) -> usize {
        self as usize
    }

    fn contains(self, c: char) -> bool {
        match self {
            CharSet::Digits => c.is_ascii_digit(),
            CharSet::Letters => c.is_ascii_uppercase(),
            CharSet::Roman => "MDCLXVI".contains(c),
        }
    }
}

/// What one part of a picture string matches (jsonata-js `generateMatcher`).
#[derive(Debug, Clone)]
enum Matcher {
    /// The given text, canonicalized.
    Literal(Vec<char>),
    /// A number in words.
    Words,
    /// A run of `chars`: the longest that lets the rest of the timestamp
    /// match, or exactly `width` of them, optionally followed by an ordinal
    /// suffix. A timezone has a prefix before the run, and its run can be
    /// split by a separator (hours and minutes).
    Run {
        chars: CharSet,
        /// The characters allowed at each position of the prefix.
        prefix: &'static [&'static str],
        separator: Option<Vec<char>>,
        /// `[Z0]` and `[Z01]`: optional minutes, as two digits after a colon.
        minutes: bool,
        /// Zero (or none) is no fixed width; a negative width matches nothing.
        width: Option<i64>,
        ordinal: bool,
    },
}

#[derive(Debug, Clone)]
struct MatcherPart {
    matcher: Matcher,
    parse: Option<ParseKind>,
}

/// Mirrors `canonicalize`: each character in upper case, as a regex with the
/// `i` flag compares them, unless that would change its length or make a
/// non-ASCII character ASCII.
fn canonical_char(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_uppercase();
    }
    if c.len_utf16() == 2 {
        // jsonata-js works on UTF-16 code units, and a surrogate has no
        // upper case, so a character outside the BMP never changes.
        return c;
    }
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(u), None) if !u.is_ascii() && u.len_utf16() == c.len_utf16() => u,
        _ => c,
    }
}

fn canonicalize(s: &str) -> Vec<char> {
    s.chars().map(canonical_char).collect()
}

/// The words a number in words can be made of, in upper case and grouped by
/// their first letter, in the order jsonata-js tries them: the insertion
/// order of its `wordValues` object, then "and".
fn words_by_initial() -> &'static HashMap<char, Vec<Vec<char>>> {
    static MAP: OnceLock<HashMap<char, Vec<Vec<char>>>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut order: Vec<String> = Vec::new();
        let mut add = |word: String| {
            if !order.contains(&word) {
                order.push(word);
            }
        };
        for w in FEW.iter().chain(ORDINALS.iter()) {
            add(w.to_lowercase());
        }
        for w in DECADES.iter() {
            let lword = w.to_lowercase();
            let ieth = format!("{}ieth", &lword[..lword.len() - 1]);
            add(lword);
            add(ieth);
        }
        add("hundredth".to_string());
        for w in MAGNITUDES.iter() {
            let lword = w.to_lowercase();
            add(lword.clone());
            add(format!("{lword}th"));
        }
        add("and".to_string());
        let mut map: HashMap<char, Vec<Vec<char>>> = HashMap::new();
        for word in order {
            let upper: Vec<char> = word.to_uppercase().chars().collect();
            map.entry(upper[0]).or_default().push(upper);
        }
        map
    })
}

const WORD_SEPARATORS: &str = "-, ";
const ORDINAL_SUFFIXES: [&str; 4] = ["TH", "ST", "ND", "RD"];

fn integer_matcher(fmt: &IntegerFormat) -> Result<Matcher, DateTimeError> {
    let run = |chars| Matcher::Run {
        chars,
        prefix: &[],
        separator: None,
        minutes: false,
        width: None,
        ordinal: false,
    };
    Ok(match fmt.primary {
        PrimaryFormat::Letters => run(CharSet::Letters),
        PrimaryFormat::Roman => run(CharSet::Roman),
        PrimaryFormat::Words => Matcher::Words,
        PrimaryFormat::Decimal => Matcher::Run {
            chars: CharSet::Digits,
            prefix: &[],
            separator: None,
            minutes: false,
            width: fmt.parse_width,
            ordinal: fmt.ordinal,
        },
        PrimaryFormat::Sequence => {
            return Err(coded("D3130", fmt.token.clone().unwrap_or_default()));
        }
    })
}

fn build_matcher_parts(spec: &[PictureItem]) -> Result<Vec<MatcherPart>, DateTimeError> {
    let mut parts = Vec::with_capacity(spec.len());
    for item in spec {
        let part = match item {
            PictureItem::Literal(lit) => MatcherPart {
                matcher: Matcher::Literal(canonicalize(lit)),
                parse: None,
            },
            PictureItem::Marker(m) if m.component == 'Z' || m.component == 'z' => {
                let fmt = m
                    .integer_format
                    .as_ref()
                    .ok_or_else(|| coded("D3134", m.component.to_string()))?;
                let separator = fmt.regular.then(|| fmt.grouping_separators[0].clone());
                let mut width = None;
                if fmt.parse_width.is_some() {
                    // Directly followed by another number in digits: the
                    // number of digits (after any separator) is fixed, if the
                    // timezone is always formatted with that many.
                    if let Some(sep) = &separator {
                        width = Some(sep.position as i64);
                    } else if fmt.mandatory_digits == 2 || fmt.mandatory_digits == 4 {
                        width = Some(fmt.mandatory_digits as i64);
                    }
                }
                MatcherPart {
                    matcher: Matcher::Run {
                        chars: CharSet::Digits,
                        prefix: if m.component == 'z' {
                            &["G", "M", "T", "-+"]
                        } else {
                            &["-+"]
                        },
                        separator: separator
                            .as_ref()
                            .map(|sep| vec![canonical_char(sep.character)]),
                        minutes: separator.is_none() && fmt.mandatory_digits <= 2,
                        width,
                        ordinal: false,
                    },
                    parse: Some(ParseKind::Timezone {
                        has_gmt: m.component == 'z',
                        separator: separator.map(|sep| sep.character.to_string()),
                    }),
                }
            }
            PictureItem::Marker(m) if m.component == 'f' => {
                let mut width = None;
                if let Some(fmt) = &m.integer_format {
                    if fmt.parse_width.is_some() && fmt.primary == PrimaryFormat::Decimal {
                        if let (_, _, Some((min_digits, max_digits))) =
                            fractional_seconds_format(m)?
                        {
                            if min_digits == max_digits {
                                width = Some(min_digits as i64);
                            }
                        }
                    }
                }
                MatcherPart {
                    matcher: Matcher::Run {
                        chars: CharSet::Digits,
                        prefix: &[],
                        separator: None,
                        minutes: false,
                        width,
                        ordinal: false,
                    },
                    parse: Some(ParseKind::Fraction),
                }
            }
            PictureItem::Marker(m) => match &m.integer_format {
                Some(fmt) => MatcherPart {
                    matcher: integer_matcher(fmt)?,
                    parse: Some(ParseKind::Integer(fmt.clone())),
                },
                None => {
                    // A month or day name, or am/pm. Any other component
                    // can't be a name (D3133).
                    if !"MxFP".contains(m.component) {
                        return Err(coded("D3133", m.component.to_string()));
                    }
                    MatcherPart {
                        matcher: Matcher::Run {
                            chars: CharSet::Letters,
                            prefix: &[],
                            separator: None,
                            minutes: false,
                            width: None,
                            ordinal: false,
                        },
                        parse: Some(ParseKind::Name {
                            component: m.component,
                            width_max: m.width.as_ref().and_then(|w| w.max),
                            width_negative: m.width.as_ref().is_some_and(|w| w.max_negative),
                        }),
                    }
                }
            },
        };
        parts.push(part);
    }
    Ok(parts)
}

/// `Number.parseInt` of a string, NaN when it has no leading digits.
fn parse_int_or_nan(s: &str) -> f64 {
    js_parse_int(s)
}

/// The value of a component from the text that matched it, `None` where
/// jsonata-js's `parse` gives undefined (an unknown name). Values are
/// JavaScript numbers, so NaN is possible.
fn parse_component_value(kind: &ParseKind, captured: &str) -> Option<f64> {
    match kind {
        ParseKind::Timezone { has_gmt, separator } => {
            let chars: Vec<char> = captured.chars().collect();
            let chars = if *has_gmt { &chars[3..] } else { &chars[..] };
            // The sign applies to both the hours and the minutes, including
            // offsets such as -00:30 (jsonata-js #856).
            let sign = if chars.first() == Some(&'-') {
                -1.0
            } else {
                1.0
            };
            let value: String = chars.iter().skip(1).collect();
            let (hours, minutes) = if let Some(sep) = separator {
                // The separator could be the same as the sign, and it is
                // matched case-insensitively.
                let canon_value = canonicalize(&value);
                let canon_sep = canonicalize(sep);
                let position = canon_value
                    .windows(canon_sep.len())
                    .position(|w| w == canon_sep.as_slice())
                    .unwrap_or(0);
                let value_chars: Vec<char> = value.chars().collect();
                let head: String = value_chars[..position].iter().collect();
                let tail: String = value_chars
                    [(position + canon_sep.len()).min(value_chars.len())..]
                    .iter()
                    .collect();
                (parse_int_or_nan(&head), parse_int_or_nan(&tail))
            } else if let Some(colon) = value.find(':') {
                (
                    parse_int_or_nan(&value[..colon]),
                    parse_int_or_nan(&value[colon + 1..]),
                )
            } else if value.len() <= 2 {
                (parse_int_or_nan(&value), 0.0)
            } else {
                let split = value.len() - 2;
                (
                    parse_int_or_nan(&value[..split]),
                    parse_int_or_nan(&value[split..]),
                )
            };
            Some(sign * (hours * 60.0 + minutes))
        }
        ParseKind::Fraction => {
            let truncated: String = captured.chars().take(3).collect();
            let fraction: f64 = format!("0.{truncated}").parse().unwrap_or(f64::NAN);
            Some(fraction * 1000.0)
        }
        ParseKind::Integer(fmt) => Some(parse_integer_format_value(fmt, captured)),
        ParseKind::Name {
            component,
            width_max,
            width_negative,
        } => {
            if *width_negative {
                return None;
            }
            name_lookup(*component, captured, *width_max)
        }
    }
}

/// Mirrors `matchParts`: match a timestamp against the parts of a picture
/// string, finding the same match as the regex made by concatenating a
/// capturing group for each part. Like a backtracking regex engine, it tries
/// the possible matches of each part in turn, in the same order, until the
/// rest of the timestamp matches the rest of the parts. Unlike a regex, it
/// never tries to match the rest of the parts from the same part and
/// position twice, so it takes time proportional to the length of the
/// timestamp times the number of parts, however the picture string is
/// constructed (jsonata-js #869). Returns the text matching each part.
fn match_parts(parts: &[MatcherPart], timestamp: &[char]) -> Option<Vec<String>> {
    const FAILED: u8 = 1;
    const VISITED: u8 = 2;

    let count = parts.len();
    let length = timestamp.len();
    let stride = length + 1;
    if count == 0 {
        return (length == 0).then(Vec::new);
    }
    let text: Vec<char> = timestamp.iter().map(|&c| canonical_char(c)).collect();
    let mut memo = vec![0u8; stride * count];
    // For each part that matches a range of positions, the lowest position
    // tried, plus one (zero if none tried yet).
    let mut lowest_tried = vec![0usize; stride * count];

    // Where each literal occurs, found in one pass (Knuth-Morris-Pratt).
    let literal_starts: Vec<Option<Vec<bool>>> = parts
        .iter()
        .map(|part| match &part.matcher {
            Matcher::Literal(literal) => Some(literal_positions(literal, &text, stride)),
            _ => None,
        })
        .collect();

    // The position after the run of characters from each set that starts at
    // each position.
    let run_ends: Vec<Vec<usize>> = CharSet::ALL
        .iter()
        .map(|set| {
            let mut ends = vec![0usize; stride];
            ends[length] = length;
            for ii in (0..length).rev() {
                ends[ii] = if set.contains(text[ii]) {
                    ends[ii + 1]
                } else {
                    ii
                };
            }
            ends
        })
        .collect();
    let run_end = |set: CharSet, pos: usize| run_ends[set.index()][pos];
    let starts_with = |s: &[char], pos: usize| text.get(pos..pos + s.len()) == Some(s);
    let words = words_by_initial();
    let next_words = |pos: usize| -> Vec<Vec<char>> {
        match text.get(pos) {
            Some(ch) => match words.get(ch) {
                Some(list) => list.clone(),
                None if WORD_SEPARATORS.contains(*ch) => vec![vec![*ch]],
                None => Vec::new(),
            },
            None => Vec::new(),
        }
    };

    // One cursor per part, stepping through the positions where it can end
    // in the order a regex would try them. Only one cursor per part is in
    // use at a time.
    enum Cursor {
        Words {
            positions: Vec<usize>,
            options: Vec<Vec<Vec<char>>>,
            tried: Vec<usize>,
        },
        Range {
            low: usize,
            high: isize,
            key: Option<usize>,
            first: Option<usize>,
        },
    }
    let mut cursors: Vec<Cursor> = parts
        .iter()
        .map(|part| match part.matcher {
            Matcher::Words => Cursor::Words {
                positions: Vec::new(),
                options: Vec::new(),
                tried: Vec::new(),
            },
            _ => Cursor::Range {
                low: 0,
                high: -1,
                key: None,
                first: None,
            },
        })
        .collect();

    let start_cursor = |cursors: &mut [Cursor],
                        memo: &mut [u8],
                        lowest_tried: &[usize],
                        index: usize,
                        pos: usize| {
        let base = index * stride;
        match &mut cursors[index] {
            Cursor::Words {
                positions,
                options,
                tried,
            } => {
                positions.clear();
                options.clear();
                tried.clear();
                if memo[base + pos] & VISITED == 0 {
                    positions.push(pos);
                    options.push(next_words(pos));
                    tried.push(0);
                }
            }
            Cursor::Range {
                low,
                high,
                key,
                first,
            } => {
                *low = 0;
                *high = -1;
                *key = None;
                *first = None;
                fn only(end: usize, low: &mut usize, high: &mut isize) {
                    *low = end;
                    *high = end as isize;
                }
                match &parts[index].matcher {
                    Matcher::Literal(literal) => {
                        if literal_starts[index].as_ref().is_some_and(|s| s[pos]) {
                            only(pos + literal.len(), low, high);
                        }
                    }
                    Matcher::Words => unreachable!("a words part has a words cursor"),
                    Matcher::Run {
                        chars,
                        prefix,
                        separator,
                        minutes,
                        width,
                        ordinal,
                    } => {
                        let mut from = pos;
                        for allowed in prefix.iter() {
                            if from == length || !allowed.contains(text[from]) {
                                return;
                            }
                            from += 1;
                        }
                        if let Some(sep) = separator {
                            // The separator isn't one of the characters before
                            // it, so they must all be before it.
                            let head = run_end(*chars, from);
                            if head == from || !starts_with(sep, head) {
                                return;
                            }
                            from = head + sep.len();
                        }
                        let mut end = run_end(*chars, from);
                        if end == from {
                            return;
                        }
                        let width = width.filter(|w| *w != 0);
                        if let Some(w) = width {
                            // A width that isn't positive (from a width
                            // modifier such as [Y,1--2]) can't match anything.
                            if w < 0 || from + w as usize > end {
                                return;
                            }
                            end = from + w as usize;
                        }
                        if *minutes
                            && text.get(end) == Some(&':')
                            && run_end(CharSet::Digits, (end + 1).min(length)) >= end + 3
                        {
                            // The optional minutes of a timezone are tried
                            // before the hours on their own, as the regex
                            // (?::[0-9]{2})? would.
                            *first = Some(end + 3);
                        }
                        if width.is_none() && !ordinal {
                            let k = base + end;
                            *key = Some(k);
                            *low = from + 1;
                            *high = if lowest_tried[k] == 0 {
                                end as isize
                            } else {
                                (end as isize).min(lowest_tried[k] as isize - 2)
                            };
                            return;
                        }
                        // A fixed number of characters, or all of them if
                        // followed by an ordinal suffix (which isn't one of them).
                        if !ordinal {
                            only(end, low, high);
                        } else {
                            let suffix: String = text
                                .get(end..(end + 2).min(length))
                                .map(|s| s.iter().collect())
                                .unwrap_or_default();
                            if ORDINAL_SUFFIXES.contains(&suffix.as_str()) {
                                only(end + 2, low, high);
                            }
                        }
                    }
                }
            }
        }
    };

    let next_end = |cursors: &mut [Cursor],
                    memo: &mut [u8],
                    lowest_tried: &mut [usize],
                    index: usize|
     -> Option<usize> {
        let base = index * stride;
        match &mut cursors[index] {
            Cursor::Words {
                positions,
                options,
                tried,
            } => {
                // A number in words matches like the regex
                // (?:word1|word2|...|[-, ])+, which tries to continue with
                // each word in turn before ending at the current position: a
                // depth-first search through the words, searching from each
                // position only once.
                while let Some(top) = positions.len().checked_sub(1) {
                    if tried[top] < options[top].len() {
                        let word = options[top][tried[top]].clone();
                        tried[top] += 1;
                        let pos = positions[top] + word.len();
                        if starts_with(&word, positions[top]) && memo[base + pos] & VISITED == 0 {
                            memo[base + pos] |= VISITED;
                            positions.push(pos);
                            options.push(next_words(pos));
                            tried.push(0);
                        }
                    } else {
                        let end = positions.pop().expect("non-empty");
                        options.pop();
                        tried.pop();
                        if top > 0 {
                            // The number has to have at least one word.
                            return Some(end);
                        }
                    }
                }
                None
            }
            Cursor::Range {
                low,
                high,
                key,
                first,
            } => {
                if let Some(end) = first.take() {
                    return Some(end);
                }
                if *high < *low as isize {
                    return None;
                }
                if let Some(k) = key {
                    lowest_tried[*k] = (*high + 1) as usize;
                }
                let end = *high as usize;
                *high -= 1;
                Some(end)
            }
        }
    };

    // Search for the end of each part in turn, backtracking to the previous
    // part when the current one can't be matched.
    let mut starts = vec![0usize; count];
    let mut ends = vec![0usize; count];
    start_cursor(&mut cursors, &mut memo, &lowest_tried, 0, 0);
    let mut depth = 1;
    while depth > 0 {
        let index = depth - 1;
        match next_end(&mut cursors, &mut memo, &mut lowest_tried, index) {
            None => {
                memo[index * stride + starts[index]] |= FAILED;
                depth -= 1;
            }
            Some(end) if index == count - 1 => {
                if end == length {
                    ends[index] = end;
                    return Some(
                        starts
                            .iter()
                            .zip(&ends)
                            .map(|(&s, &e)| timestamp[s..e].iter().collect())
                            .collect(),
                    );
                }
            }
            Some(end) => {
                if memo[(index + 1) * stride + end] & FAILED == 0 {
                    ends[index] = end;
                    starts[index + 1] = end;
                    start_cursor(&mut cursors, &mut memo, &lowest_tried, index + 1, end);
                    depth = index + 2;
                }
            }
        }
    }
    None
}

/// The positions where `literal` occurs in `text` (Knuth-Morris-Pratt), as
/// a flag for each of the `stride` positions.
fn literal_positions(literal: &[char], text: &[char], stride: usize) -> Vec<bool> {
    let mut starts = vec![false; stride];
    if literal.is_empty() {
        return starts;
    }
    let mut overlap = vec![0usize; literal.len()];
    let mut kk = 0;
    for ii in 1..literal.len() {
        while kk > 0 && literal[ii] != literal[kk] {
            kk = overlap[kk - 1];
        }
        if literal[ii] == literal[kk] {
            kk += 1;
        }
        overlap[ii] = kk;
    }
    kk = 0;
    for (ii, &c) in text.iter().enumerate() {
        while kk > 0 && c != literal[kk] {
            kk = overlap[kk - 1];
        }
        if c == literal[kk] {
            kk += 1;
        }
        if kk == literal.len() {
            starts[ii + 1 - kk] = true;
            kk = overlap[kk - 1];
        }
    }
    starts
}

/// Returns `f64` (matching JS Number) since the `words` format can parse magnitudes far
/// beyond `i64::MAX` (e.g. "ten billion trillion trillion trillion" -> 1e46).
/// Strip an ordinal suffix, grouping separators and a non-ASCII zero digit,
/// leaving the bare digit string the reference's `matcher.parse` works on.
fn decimal_digits(fmt: &IntegerFormat, captured: &str) -> String {
    let mut digits = captured.to_string();
    if fmt.ordinal && digits.len() >= 2 {
        digits.truncate(digits.len() - 2);
    }
    // Every grouping separator of the picture, regular or not, in one pass
    // (jsonata-js 2.3.0 `removeGroupingSeparators`; 2.2 stripped only ','
    // from a regular picture).
    if !fmt.grouping_separators.is_empty() {
        let separators: std::collections::HashSet<char> = fmt
            .grouping_separators
            .iter()
            .map(|sep| sep.character)
            .collect();
        digits.retain(|c| !separators.contains(&c));
    }
    if fmt.zero_code != 0x30 {
        digits = digits
            .chars()
            // Only digits of the picture's own family are mapped to ASCII;
            // anything else (an ASCII digit, say) is left alone instead of
            // underflowing into an invalid code point (as jsonata-js 2.3.0's
            // `toAsciiDigits` does).
            .map(|c| {
                let code = c as u32;
                if (fmt.zero_code..=fmt.zero_code + 9).contains(&code) {
                    char::from_u32(code - fmt.zero_code + 0x30).unwrap_or(c)
                } else {
                    c
                }
            })
            .collect();
    }
    digits
}

/// JavaScript's `parseInt(digits, 10)`: read an optional sign and the leading
/// run of digits, ignore whatever follows, and give NaN when there is no
/// leading digit at all.
fn js_parse_int(digits: &str) -> f64 {
    let s = digits.trim_start();
    let mut chars = s.char_indices();
    let mut end = 0;
    let mut started = false;
    if let Some((_, c)) = chars.clone().next() {
        if c == '+' || c == '-' {
            end = c.len_utf8();
            chars.next();
        }
    }
    for (i, c) in chars {
        if c.is_ascii_digit() {
            started = true;
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    if !started {
        return f64::NAN;
    }
    s[..end].parse::<f64>().unwrap_or(f64::NAN)
}

/// A JavaScript number, as jsonata-js's integer matcher `parse` gives it:
/// NaN for digits `parseInt` can't read or for words it doesn't know.
fn parse_integer_format_value(fmt: &IntegerFormat, captured: &str) -> f64 {
    match fmt.primary {
        PrimaryFormat::Letters => {
            letters_to_decimal(captured, if fmt.case == TCase::Upper { 'A' } else { 'a' })
        }
        PrimaryFormat::Roman => {
            let upper = if fmt.case == TCase::Upper {
                captured.to_string()
            } else {
                captured.to_uppercase()
            };
            roman_to_decimal(&upper)
        }
        PrimaryFormat::Words => words_to_number(&captured.to_lowercase()).unwrap_or(f64::NAN),
        PrimaryFormat::Decimal => js_parse_int(&decimal_digits(fmt, captured)),
        // generateMatcher raises D3130 for a sequence before anything is parsed.
        PrimaryFormat::Sequence => f64::NAN,
    }
}

/// The value of a month or day name, or am/pm, or `None` (undefined in
/// jsonata-js) for text that isn't one. The lookup is case-sensitive, though
/// the timestamp is matched case-insensitively, as in jsonata-js.
fn name_lookup(component: char, captured: &str, width_max: Option<usize>) -> Option<f64> {
    let key = |name: &str| -> String {
        match width_max {
            Some(w) if w > 0 => name.chars().take(w).collect(),
            _ => name.to_string(),
        }
    };
    match component {
        // jsonata-js builds a lookup object in order, so when truncated names
        // collide the last one wins: "Ju" is July.
        'M' | 'x' => MONTHS
            .iter()
            .rposition(|name| key(name) == captured)
            .map(|i| (i + 1) as f64),
        'F' => DAYS
            .iter()
            .enumerate()
            .skip(1)
            .rev()
            .find(|(_, name)| key(name) == captured)
            .map(|(i, _)| i as f64),
        'P' => match captured {
            "am" | "AM" => Some(0.0),
            "pm" | "PM" => Some(1.0),
            _ => None,
        },
        _ => None,
    }
}

/// The largest match (picture parts times timestamp positions) attempted
/// when parsing a timestamp (jsonata-js 2.3.0 `maxMatchSize`).
const MAX_MATCH_SIZE: usize = 1_000_000;

/// Parses a string containing a timestamp as specified by a date/time picture string
/// (mirrors datetime.js's `parseDateTime`). Returns `Ok(None)` (meaning "undefined") if
/// the timestamp doesn't match the picture, or if the picture has no markers at all.
fn parse_date_time(timestamp: &str, picture: &str, now: i64) -> Result<Option<f64>, DateTimeError> {
    let spec = analyse_date_time_picture(picture)?;

    // jsonata-js 2.3.0 (`analyseDateTimePicture`'s `cannotMatch`): a
    // timestamp needs every character of each literal and at least one per
    // marker, and matching costs time and memory proportional to the number
    // of picture parts times the timestamp length. A timestamp that is too
    // short, or a match that would be too large, cannot match, so the result
    // is undefined. The picture is analysed first, so an invalid one is still
    // an error.
    // Lengths in UTF-16 code units, as JavaScript's `length` counts them.
    let max_length = timestamp.encode_utf16().count();
    let timestamp: Vec<char> = timestamp.chars().collect();
    let min_length: usize = spec
        .iter()
        .map(|item| match item {
            PictureItem::Literal(lit) => lit.encode_utf16().count(),
            PictureItem::Marker(_) => 1,
        })
        .sum();
    if min_length > max_length || spec.len().saturating_mul(max_length + 1) > MAX_MATCH_SIZE {
        return Ok(None);
    }

    let parts = build_matcher_parts(&spec)?;
    let Some(info) = match_parts(&parts, &timestamp) else {
        return Ok(None);
    };

    // The component values, as JavaScript numbers. A component whose text
    // gave undefined (an unknown name) is left out, as if it wasn't given.
    let mut components: HashMap<char, f64> = HashMap::new();
    let mut any_parsed = false;
    for ((part, item), text) in parts.iter().zip(&spec).zip(&info) {
        let (Some(kind), PictureItem::Marker(m)) = (&part.parse, item) else {
            continue;
        };
        any_parsed = true;
        // `components[c] = parse(text)`: a later undefined replaces an
        // earlier value.
        match parse_component_value(kind, text) {
            Some(value) => components.insert(m.component, value),
            None => components.remove(&m.component),
        };
    }
    if !any_parsed {
        return Ok(None);
    }

    // jsonata-js builds the masks with `shift(components[part])`, which is
    // truthy-tested: a component that parsed as 0 or NaN (hour 0, "am", day
    // 0) counts as not given when choosing the date and time types.
    let mask_of = |order: &str| -> u8 {
        let mut mask = 0u8;
        for ch in order.chars() {
            let given = components
                .get(&ch)
                .is_some_and(|value| *value != 0.0 && !value.is_nan());
            mask = (mask << 1) | given as u8;
        }
        mask
    };
    let is_type = |type_: u8, mask: u8| (mask & !type_) == 0 && (mask & type_) != 0;

    let dmask = mask_of("YXMxWwdD");
    let date_a = is_type(0b1010_0001, dmask);
    let date_b = !date_a && is_type(0b1000_0010, dmask);
    let date_c = is_type(0b0101_0100, dmask);
    let date_d = !date_c && is_type(0b0100_1000, dmask);

    let tmask = mask_of("PHhmsf");
    let time_a = is_type(0b010111, tmask);
    let time_b = !time_a && is_type(0b101111, tmask);

    let date_comps = if date_b {
        "YD"
    } else if date_c {
        "XxwF"
    } else if date_d {
        "XWF"
    } else {
        "YMD"
    };
    let time_comps = if time_b { "Phmsf" } else { "Hmsf" };

    // From the most significant part to the least: an unspecified part before
    // the specified block defaults to now, one after it to zero (or 1 for a
    // month or day), and a gap inside it is an error.
    let mut start_specified = false;
    let mut end_specified = false;
    for part in date_comps.chars().chain(time_comps.chars()) {
        match components.entry(part) {
            std::collections::hash_map::Entry::Occupied(_) => {
                start_specified = true;
                if end_specified {
                    return Err(coded(
                        "D3136",
                        "date/time components are inconsistent (gap between specified fields)",
                    ));
                }
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                if start_specified {
                    e.insert(if "MDd".contains(part) { 1.0 } else { 0.0 });
                    end_specified = true;
                } else {
                    e.insert(get_date_time_fragment(now, part) as f64);
                }
            }
        }
    }

    // dateC ({Y, x, w, F}) and dateD ({X, W, F}) are not supported for
    // parsing, as in jsonata-js.
    if date_c || date_d {
        return Err(coded(
            "D3136",
            "ISO week-numbering year/month parsing is not supported",
        ));
    }

    let get = |c: char| components.get(&c).copied().unwrap_or(f64::NAN);
    let mut month0 = match components.get(&'M') {
        Some(&m) if m > 0.0 => m - 1.0,
        _ => 0.0,
    };
    let mut day = get('D');
    let year = get('Y');
    if date_b {
        // The month and day `d - 1` days on from 1 January, kept with the
        // parsed year, so a day past the year's end lands in the same year.
        // If 1 January or the derived date is past JavaScript's range, the
        // date is invalid and the result NaN.
        let derived = js_integer(year)
            .and_then(|y| date_utc(y, 0, 1, 0, 0, 0, 0))
            .and_then(|first_jan| {
                let offset_millis = js_integer(get('d'))?
                    .checked_sub(1)?
                    .checked_mul(MILLIS_PER_DAY)?;
                first_jan.checked_add(offset_millis)
            })
            .filter(|derived| (-MAX_DATE_MILLIS..=MAX_DATE_MILLIS).contains(derived));
        let Some(derived) = derived else {
            return Ok(Some(f64::NAN));
        };
        let (_, m, d) = CivilDate::from_millis(derived).ymd();
        month0 = (m - 1) as f64;
        day = d as f64;
    }
    let hour = if time_b {
        // 12-hour to 24-hour.
        let h = get('h');
        let hour = if h == 12.0 { 0.0 } else { h };
        if components.get(&'P') == Some(&1.0) {
            hour + 12.0
        } else {
            hour
        }
    } else {
        get('H')
    };

    // `Date.UTC`, which gives NaN for a NaN field or a date past JavaScript's
    // range, and which jsonata-js returns as it is.
    let fields = [year, month0, day, hour, get('m'), get('s'), get('f')];
    let integers: Option<Vec<i64>> = fields.iter().map(|v| js_integer(*v)).collect();
    let Some(millis) = integers.and_then(|f| date_utc(f[0], f[1], f[2], f[3], f[4], f[5], f[6]))
    else {
        return Ok(Some(f64::NAN));
    };
    // `if (components.Z || components.z) millis -= (components.Z ||
    // components.z) * 60 * 1000`, in JavaScript numbers: a zero or NaN Z falls
    // through to z, and nothing clips the result.
    let truthy = |v: &&f64| **v != 0.0 && !v.is_nan();
    let offset = components
        .get(&'Z')
        .filter(truthy)
        .or_else(|| components.get(&'z').filter(truthy))
        .copied()
        .unwrap_or(0.0);
    Ok(Some(millis as f64 - offset * 60.0 * 1000.0))
}

/// A JavaScript number as `Date.UTC` reads it (ToIntegerOrInfinity): the
/// integer part, or `None` for NaN, infinity or a magnitude too large for any
/// date.
fn js_integer(value: f64) -> Option<i64> {
    (value.is_finite() && value.abs() < 9e15).then(|| value.trunc() as i64)
}

/// JavaScript's `Date.UTC(year, month0, day, hour, minute, second, millis)`:
/// milliseconds since the epoch, carrying any field past its range into the
/// next (ECMAScript's MakeDay and MakeTime), or `None` past JavaScript's dates
/// (TimeClip), where `Date.UTC` gives NaN.
fn date_utc(
    year: i64,
    month0: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    millis: i64,
) -> Option<i64> {
    let year = js_utc_year(year).checked_add(month0.div_euclid(12))?;
    if !(-MAX_CIVIL_YEAR..=MAX_CIVIL_YEAR).contains(&year) {
        return None;
    }
    let first_of_month = days_from_civil(year, month0.rem_euclid(12) + 1, 1);
    let days = first_of_month.checked_add(day.checked_sub(1)?)?;
    let time = hour
        .checked_mul(3_600_000)?
        .checked_add(minute.checked_mul(60_000)?)?
        .checked_add(second.checked_mul(1000)?)?
        .checked_add(millis)?;
    let total = days.checked_mul(MILLIS_PER_DAY)?.checked_add(time)?;
    (-MAX_DATE_MILLIS..=MAX_DATE_MILLIS)
        .contains(&total)
        .then_some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The code of a coded datetime error, for the 2.2.17 guard tests.
    fn error_code(result: Result<JValue, DateTimeError>) -> &'static str {
        match result {
            Err(DateTimeError::Coded { code, .. }) => code,
            other => panic!("expected a coded error, got {other:?}"),
        }
    }

    #[test]
    fn width_modifier_over_100_is_d3142() {
        for picture in [
            "[Y,101]",
            "[Y,600000000]",
            "[f,150000000]",
            "[Y,2-101]",
            "[D,99999999999999999999]",
        ] {
            assert_eq!(
                error_code(from_millis_with_picture(0.0, Some(picture), None)),
                "D3142",
                "{picture}"
            );
        }
        assert_eq!(
            error_code(to_millis_with_picture("1970", "[Y,101]")),
            "D3142"
        );
    }

    #[test]
    fn width_modifier_of_100_does_not_overflow_the_year_modulus() {
        let formatted = from_millis_with_picture(0.0, Some("[Y,100]"), None).unwrap();
        assert_eq!(formatted, JValue::string(format!("{}1970", "0".repeat(96))));
    }

    #[test]
    fn roman_numerals_above_1e7_are_decimal() {
        assert_eq!(
            format_integer(1e8, "I").unwrap(),
            JValue::string("100000000")
        );
        assert_eq!(
            format_integer(-1e15, "i").unwrap(),
            JValue::string("-1000000000000000")
        );
        assert_eq!(
            format_integer(1e20, "I").unwrap(),
            JValue::string("100000000000000000000")
        );
        assert_eq!(
            format_integer(1999.0, "I").unwrap(),
            JValue::string("MCMXCIX")
        );
    }

    #[test]
    fn digits_outside_the_picture_family_do_not_panic() {
        assert_eq!(
            parse_integer("12", "\u{661}").unwrap(),
            JValue::Number(12.0)
        );
        assert_eq!(
            parse_integer("\u{661}\u{662}", "\u{661}").unwrap(),
            JValue::Number(12.0)
        );
    }

    #[test]
    fn fractional_seconds_as_a_name_is_d3133() {
        assert_eq!(
            error_code(from_millis_with_picture(0.0, Some("[fN]"), None)),
            "D3133"
        );
    }

    #[test]
    fn unmatchable_timestamps_are_undefined_without_matching() {
        let markers = "[Y]1".repeat(100_000);
        let timestamp = format!("{}x", "1".repeat(99_999));
        assert_eq!(
            to_millis_with_picture(&timestamp, &markers).unwrap(),
            JValue::Undefined
        );
        let literal = "a".repeat(10_000_000);
        assert_eq!(
            to_millis_with_picture("x", &literal).unwrap(),
            JValue::Undefined
        );
        assert_eq!(
            to_millis_with_picture("1", "[Y][M]").unwrap(),
            JValue::Undefined
        );
    }

    /// The date `year`-`month`-`day`, or `None` if there is no such day.
    fn from_ymd(year: i64, month: i64, day: i64) -> Option<CivilDate> {
        let days_in_month = match month {
            2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            1..=12 => 31,
            _ => return None,
        };
        (1..=days_in_month).contains(&day).then(|| CivilDate {
            days: days_from_civil(year, month, day),
        })
    }

    /// `CivilDate` against dates whose day numbers and weekdays are fixed
    /// facts, and a round trip through every day of ±1,000 years around the
    /// epoch plus a stretch at either end of JavaScript's dates.
    #[test]
    fn civil_date_matches_the_gregorian_calendar() {
        for (y, m, d, days, weekday) in [
            (1970, 1, 1, 0, 4),       // Thursday
            (2000, 2, 29, 11_016, 2), // Tuesday
            (2000, 3, 1, 11_017, 3),  // Wednesday
            (1969, 12, 31, -1, 3),    // Wednesday
            (1, 1, 1, -719_162, 1),   // Monday
            (0, 2, 29, -719_469, 2),  // year 0 is a leap year
            (275_760, 9, 13, 100_000_000, 6),
            (-271_821, 4, 20, -100_000_000, 2),
        ] {
            let date = from_ymd(y, m, d).expect("a real date");
            assert_eq!(date.days, days, "{y}-{m}-{d}");
            assert_eq!(date.ymd(), (y, m, d));
            assert_eq!(date.weekday_from_monday(), weekday, "{y}-{m}-{d}");
        }
        for (y, m, d) in [
            (2001, 2, 29),
            (1900, 2, 29),
            (2018, 13, 1),
            (2018, 0, 1),
            (2018, 4, 31),
            (2018, 1, 0),
        ] {
            assert!(from_ymd(y, m, d).is_none(), "{y}-{m}-{d}");
        }
        let day_of_year =
            |y, m, d| get_date_time_fragment(from_ymd(y, m, d).unwrap().days * MILLIS_PER_DAY, 'd');
        assert_eq!(day_of_year(2016, 12, 31), 366);
        assert_eq!(day_of_year(2018, 3, 1), 60);
        // Years 0-99 count in the 1900s' calendar, as `Date.UTC` reads them.
        assert_eq!(day_of_year(0, 12, 31), 365);

        let ranges = [
            (-366_000, 366_000),
            (99_999_000, 100_000_000),
            (-100_000_000, -99_999_000),
        ];
        for (from, to) in ranges {
            let mut expected = CivilDate { days: from }.ymd();
            for days in from..=to {
                let date = CivilDate { days };
                let (y, m, d) = date.ymd();
                assert_eq!(from_ymd(y, m, d), Some(date));
                if days > from {
                    // The day after the previous one.
                    let (py, pm, pd) = expected;
                    let next = if from_ymd(py, pm, pd + 1).is_some() {
                        (py, pm, pd + 1)
                    } else if pm < 12 {
                        (py, pm + 1, 1)
                    } else {
                        (py + 1, 1, 1)
                    };
                    assert_eq!((y, m, d), next, "day {days}");
                }
                expected = (y, m, d);
            }
        }
    }

    #[test]
    fn date_utc_stays_within_javascript_dates() {
        assert_eq!(date_utc(275_760, 8, 13, 0, 0, 0, 0), Some(MAX_DATE_MILLIS));
        assert_eq!(date_utc(275_760, 8, 13, 0, 0, 0, 1), None);
        assert_eq!(
            date_utc(-271_821, 3, 20, 0, 0, 0, 0),
            Some(-MAX_DATE_MILLIS)
        );
        assert_eq!(date_utc(-271_821, 3, 19, 23, 59, 59, 999), None);
        assert_eq!(date_utc(1970, 0, 1, 24, 0, 0, 0), Some(MILLIS_PER_DAY));
        assert_eq!(date_utc(70, 0, 1, 0, 0, 0, 0), Some(0), "year 70 is 1970");
        assert_eq!(date_utc(MAX_CIVIL_YEAR + 1, 0, 1, 0, 0, 0, 0), None);
        assert_eq!(date_utc(2018, 0, 1, i64::MAX, 0, 0, 0), None);
    }

    /// `$toMillis(timestamp, picture)` as jsonata-js computes it (expected
    /// values from jsonata-js): fields past their range carry into the next,
    /// as in `Date.UTC`; a date past JavaScript's range is NaN; the offset is
    /// applied in floating point with no clipping; and a component of 0
    /// counts as not given when choosing the date and time types.
    #[test]
    fn parse_date_time_matches_jsonata_js() {
        for (timestamp, picture, expected) in [
            ("2018-01-01 24", "[Y]-[M]-[D] [H]", 1_514_851_200_000.0),
            ("2018-02-30", "[Y]-[M]-[D]", 1_519_948_800_000.0),
            ("1999 13", "[Y] [M]", 946_684_800_000.0),
            ("2018-400", "[Y]-[d]", 1_517_702_400_000.0),
            ("2018-0", "[Y]-[d]", 1_514_764_800_000.0),
            (
                "28/25/98 2:2 am",
                "[D]/[M]/[Y] [h]:[m] [P]",
                949_024_920_000.0,
            ),
            (
                "2018-01-01 +00:00 GMT+05:00",
                "[Y]-[M]-[D] [Z] [z]",
                1_514_746_800_000.0,
            ),
            (
                "275760-09-13T00:00:00-05:00",
                "[Y]-[M]-[D]T[H]:[m]:[s][Z01:01]",
                8_640_000_018_000_000.0,
            ),
            (
                "2018-01-01T00:00:00+0099999999999999999",
                "[Y]-[M]-[D]T[H]:[m]:[s][Z0000]",
                // jsonata-js 2.3.0 takes the last two digits as the minutes
                // (2.2 split after the third character).
                -3.599999998485237e21,
            ),
            (
                "2018-01-01T00:00:00+9999999999999999:00",
                "[Y]-[M]-[D]T[H]:[m]:[s][Z01:01]",
                -3.5999999998485235e22,
            ),
        ] {
            assert_eq!(
                parse_date_time(timestamp, picture, 0).unwrap(),
                Some(expected),
                "{timestamp} / {picture}"
            );
        }
        for (timestamp, picture) in [
            ("280000", "[Y]"),
            ("2018-100000000", "[Y]-[d]"),
            ("99999999999999999999 pm", "[h] [P]"),
        ] {
            let millis = parse_date_time(timestamp, picture, 0).unwrap().unwrap();
            assert!(millis.is_nan(), "{timestamp} / {picture}: {millis}");
        }
        // An hour of 0 counts as not given, so the minute after it is a gap.
        let err = parse_date_time("39/13/1927 0:33 am", "[D]/[M]/[Y] [h]:[m] [P]", 0)
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("D3136"), "{err}");
    }

    /// `$toMillis(timestamp)` as `Date.parse` reads it in jsonata-js
    /// (expected values from jsonata-js on Node).
    #[test]
    fn parse_iso8601_matches_date_parse() {
        for (timestamp, expected) in [
            ("2018-02-29", 1_519_862_400_000.0),
            ("2018-04-31", 1_525_132_800_000.0),
            ("2018-01-01T24:00:00", 1_514_851_200_000.0),
            ("2018-02-30T10:00:00Z", 1_519_984_800_000.0),
            ("2018-01-01T00:00:00+23:59", 1_514_678_460_000.0),
            ("2018-01-01T00:00:00.9999", 1_514_764_800_999.0),
            ("0099-02-29", -59_037_897_600_000.0),
            ("0000", -62_167_219_200_000.0),
            ("2018-01-01Z", 1_514_764_800_000.0),
            ("2018Z", 1_514_764_800_000.0),
            ("2018-01Z", 1_514_764_800_000.0),
        ] {
            assert_eq!(
                parse_iso8601_partial(timestamp).unwrap(),
                expected,
                "{timestamp}"
            );
        }
        for timestamp in [
            "2018-00-10",
            "2018-13-01",
            "2018-01-00",
            "2018-01-39",
            "2018-01-01T24:00:01",
            "2018-01-01T25:00:00",
            "2018-01-01T00:00:00+24:00",
            "2018-01-01.5",
            "2018-01-01+05:00",
        ] {
            assert!(
                parse_iso8601_partial(timestamp).unwrap().is_nan(),
                "{timestamp}"
            );
        }
        for timestamp in [
            "2018-23-01",
            "2018-01-41",
            "2018-01-01T30:00:00",
            "2018-1-1",
        ] {
            assert!(parse_iso8601_partial(timestamp).is_err(), "{timestamp}");
        }
    }

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
        let result = from_millis_with_picture(1.0, None, None).unwrap();
        assert!(matches!(result, JValue::String(_)));
    }

    #[test]
    fn test_to_millis_with_picture_gap_is_d3136_not_a_panic() {
        // Regression (see the design spec for reference-suite coverage gap Phase 0):
        // this used to panic (`chars[pos..end]` sliced with pos > end when a picture
        // component's width was unspecified and the input ran out of characters). Under
        // the real picture-string engine this is actually a well-defined JSONata error:
        // Y is specified, M is gap-filled (defaulted), then D reappears after the gap,
        // which is an inconsistent date specification (D3136).
        let err = to_millis_with_picture("2018-22", "[Y]-[D]").unwrap_err();
        assert!(matches!(err, DateTimeError::Coded { code: "D3136", .. }));
    }

    #[test]
    fn test_from_millis_with_picture_basic() {
        // "should format the date in ISO 8601 style" (function-fromMillis/formatDateTime)
        let result =
            from_millis_with_picture(1521801216617.0, Some("[Y0001]-[M01]-[D01]"), None).unwrap();
        assert_eq!(result, JValue::string("2018-03-23".to_string()));
    }

    #[test]
    fn test_from_millis_with_picture_roman_numeral_year() {
        // "year in roman numerals" (function-fromMillis/formatDateTime)
        let result =
            from_millis_with_picture(1521801216617.0, Some("[D1] [M01] [YI]"), None).unwrap();
        assert_eq!(result, JValue::string("23 03 MMXVIII".to_string()));
    }

    #[test]
    fn test_from_millis_with_picture_words() {
        // "year in words" (function-fromMillis/formatDateTime)
        let result = from_millis_with_picture(1521801216617.0, Some("[Yw]"), None).unwrap();
        assert_eq!(
            result,
            JValue::string("two thousand and eighteen".to_string())
        );
    }

    #[test]
    fn test_to_millis_with_picture_roman_numeral_year() {
        // "should parse year" (function-tomillis/parseDateTime)
        let result = to_millis_with_picture("MCMLXXXIV", "[YI]").unwrap();
        assert_eq!(result, JValue::Number(441763200000_i64 as f64));
    }

    #[test]
    fn test_to_millis_with_picture_words() {
        // "should parse year in words" (function-tomillis/parseDateTime)
        let result =
            to_millis_with_picture("one thousand, nine hundred and eighty-four", "[Yw]").unwrap();
        assert_eq!(result, JValue::Number(441763200000_i64 as f64));
    }

    #[test]
    fn test_number_to_words() {
        // number_to_words() returns the natural title-case form; case-folding to
        // upper/lower happens in format_integer_value() based on the picture's case.
        assert_eq!(number_to_words(2018.0, false), "Two Thousand and Eighteen");
        assert_eq!(number_to_words(23.0, true), "Twenty-Third");
    }

    #[test]
    fn test_roman_numeral_round_trip() {
        assert_eq!(decimal_to_roman(2018), "mmxviii");
        assert_eq!(roman_to_decimal("MMXVIII"), 2018.0);
    }

    #[test]
    fn test_letters_round_trip() {
        assert_eq!(decimal_to_letters(23, 'a'), "w");
        assert_eq!(letters_to_decimal("w", 'a'), 23.0);
    }
}

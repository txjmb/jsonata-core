// Date and time handling functions
// Mirrors datetime.js from the reference implementation

#![allow(clippy::unnecessary_cast)]

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::value::JValue;
use regex::{Regex, RegexBuilder};
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
    // A value that does not match does not: the reference runs the picture
    // parse without validating the input first -- its own source marks that
    // path "TODO validate input based on the matcher regex" -- so the answer
    // is whatever JavaScript's `parseInt` returns. That reads a leading run of
    // digits and gives NaN when there is none, which is why
    // `$parseInteger("12a", "000")` is 12 and `$parseInteger("abc", "000")` is
    // NaN rather than a D3136 error (#126 group 3).
    //
    // Only the decimal format goes this way. The letters/roman/words parsers
    // are shared with date-time component parsing, where a D3136 on an
    // unrecognised component is correct and load-bearing.
    if format.primary == PrimaryFormat::Decimal {
        return Ok(JValue::Number(js_parse_int(&decimal_digits(
            &format, value,
        ))));
    }
    let parsed = parse_integer_format_value(&format, value)?;
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

fn roman_value(c: char) -> i64 {
    match c {
        'M' => 1000,
        'D' => 500,
        'C' => 100,
        'L' => 50,
        'X' => 10,
        'V' => 5,
        'I' => 1,
        _ => 0,
    }
}

fn roman_to_decimal(roman: &str) -> i64 {
    let chars: Vec<char> = roman.chars().collect();
    let mut decimal = 0i64;
    let mut max = 1i64;
    for i in (0..chars.len()).rev() {
        let value = roman_value(chars[i]);
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

fn letters_to_decimal(letters: &str, a_char: char) -> i64 {
    let a_code = a_char as i64;
    let chars: Vec<char> = letters.chars().collect();
    let mut decimal = 0i64;
    for (i, c) in chars.iter().rev().enumerate() {
        decimal += (*c as i64 - a_code + 1) * 26i64.pow(i as u32);
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
    parse_width: Option<usize>,
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
    for index in 1..=indexes.len() {
        if !indexes.contains(&(index * factor)) {
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
/// must support magnitudes far beyond `i64::MAX` (e.g. 1e46). Letters/roman/decimal are
/// cast down to `i64` internally -- safe for the magnitudes they're actually specified
/// and tested for (spreadsheet columns, roman numerals, decimal digit patterns).
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
        // above ten thousand Ms are formatted as plain decimal digits.
        // f64's Display prints integers without an exponent and with the
        // shortest round-trip digits, as jsonata-js's decimal digits do, so
        // this is right above i64 too (unlike the Decimal arm, #864).
        PrimaryFormat::Roman if value > MAX_ROMAN => format!("{}", value.trunc()),
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
            // cast would saturate.
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
                    .map(|c| char::from_u32(c as u32 + format.zero_code - 0x30).unwrap())
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
                let parse_width = |s: &str| -> Result<Option<usize>, DateTimeError> {
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
                        Ok(s.parse().ok())
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
                    Some(WidthSpec { min, max }),
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
                // greedily consume digits belonging to this marker).
                if let Some(PictureItem::Marker(prev)) = spec.last_mut() {
                    if let Some(prev_fmt) = &mut prev.integer_format {
                        prev_fmt.parse_width = Some(prev_fmt.mandatory_digits);
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

fn format_timezone_component(
    offset_hours: i64,
    offset_minutes: i64,
    marker: &MarkerSpec,
) -> Result<String, DateTimeError> {
    let offset = offset_hours * 100 + offset_minutes;
    let fmt = marker
        .integer_format
        .as_ref()
        .expect("Z/z marker always has an integer_format");

    let mut component_value = if fmt.regular {
        format_integer_value(offset as f64, fmt)?
    } else {
        let num_digits = fmt.mandatory_digits;
        if num_digits == 1 || num_digits == 2 {
            let mut cv = format_integer_value(offset_hours as f64, fmt)?;
            if offset_minutes != 0 {
                cv.push(':');
                cv.push_str(&format_integer_value(
                    offset_minutes as f64,
                    &two_digit_decimal_format(),
                )?);
            }
            cv
        } else if num_digits == 3 || num_digits == 4 {
            format_integer_value(offset as f64, fmt)?
        } else {
            return Err(coded("D3134", num_digits.to_string()));
        }
    };

    if offset >= 0 {
        component_value = format!("+{component_value}");
    }
    if marker.component == 'z' {
        component_value = format!("GMT{component_value}");
    }
    if offset == 0 && marker.presentation2 == Some('t') {
        component_value = "Z".to_string();
    }

    Ok(component_value)
}

fn format_component(
    local_millis: i64,
    offset_hours: i64,
    offset_minutes: i64,
    marker: &MarkerSpec,
) -> Result<String, DateTimeError> {
    if marker.component == 'Z' || marker.component == 'z' {
        return format_timezone_component(offset_hours, offset_minutes, marker);
    }

    if marker.component == 'P' {
        // §9.8.4.7 am/pm marker
        let mut s = if get_date_time_fragment(local_millis, 'P') == 1 {
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
        let mut value = get_date_time_fragment(local_millis, marker.component);
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
        let value = get_date_time_fragment(local_millis, 'f');
        // A name presentation (`[fN]`) leaves no integer format: fractional
        // seconds can't be formatted as a name (D3133, as in jsonata-js
        // 2.3.0) -- this used to panic.
        let fmt = marker
            .integer_format
            .as_ref()
            .ok_or_else(|| coded("D3133", "f"))?;
        format_integer_value(value as f64, fmt)
    } else {
        // C (calendar) / E (era) -- always "ISO"
        Ok("ISO".to_string())
    }
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

fn parse_timezone_offset(timezone: Option<&str>) -> (i64, i64) {
    let offset = timezone.and_then(parse_leading_int).unwrap_or(0);
    let offset_hours = (offset as f64 / 100.0).floor() as i64;
    let offset_minutes = offset % 100;
    (offset_hours, offset_minutes)
}

/// Formats the date/time as specified by the XPath fn:format-dateTime function.
fn format_date_time(
    millis: f64,
    picture: Option<&str>,
    timezone: Option<&str>,
) -> Result<String, DateTimeError> {
    let (offset_hours, offset_minutes) = parse_timezone_offset(timezone);

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
    // offset too large to add up is not a date either.
    let local_millis = offset_hours
        .checked_mul(60)
        .and_then(|minutes| minutes.checked_add(offset_minutes))
        .and_then(|minutes| minutes.checked_mul(60 * 1000))
        .map(|offset_millis| (millis + offset_millis as f64).trunc())
        .filter(|shifted| shifted.abs() <= MAX_DATE_MILLIS as f64)
        .map(|shifted| shifted as i64)
        // `{:?}` keeps a huge value short (1e300, not 301 digits).
        .ok_or_else(|| DateTimeError::FormatError(format!("Invalid timestamp: {millis:?}")))?;

    let mut result = String::new();
    for item in spec {
        match item {
            PictureItem::Literal(s) => result.push_str(s),
            PictureItem::Marker(m) => {
                result.push_str(&format_component(
                    local_millis,
                    offset_hours,
                    offset_minutes,
                    m,
                )?);
            }
        }
    }
    Ok(result)
}

// ── §9.8.4 Parsing ────────────────────────────────────────────────────────────

enum ParseKind {
    Timezone {
        has_gmt: bool,
        separator: Option<char>,
    },
    Fraction,
    Integer(IntegerFormat),
    Name {
        width_max: Option<usize>,
    },
}

struct MatcherPart {
    regex: String,
    component: Option<char>,
    parse_kind: Option<ParseKind>,
}

fn integer_format_regex(fmt: &IntegerFormat) -> Result<String, DateTimeError> {
    Ok(match fmt.primary {
        PrimaryFormat::Letters => {
            if fmt.case == TCase::Upper {
                "[A-Z]+".to_string()
            } else {
                "[a-z]+".to_string()
            }
        }
        PrimaryFormat::Roman => {
            if fmt.case == TCase::Upper {
                "[MDCLXVI]+".to_string()
            } else {
                "[mdclxvi]+".to_string()
            }
        }
        PrimaryFormat::Words => {
            let mut keys: Vec<&str> = word_values().keys().map(|s| s.as_str()).collect();
            keys.sort_unstable();
            format!(r"(?:{}|and|[\-, ])+", keys.join("|"))
        }
        PrimaryFormat::Decimal => {
            let mut r = "[0-9]".to_string();
            if let Some(pw) = fmt.parse_width {
                r.push_str(&format!("{{{pw}}}"));
            } else {
                r.push('+');
            }
            if fmt.ordinal {
                r.push_str("(?:th|st|nd|rd)");
            }
            r
        }
        PrimaryFormat::Sequence => {
            return Err(coded("D3130", fmt.token.clone().unwrap_or_default()));
        }
    })
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
    if fmt.regular {
        // Faithful port of the JS reference, which strips a literal ','
        // regardless of the actual grouping-separator character.
        digits = digits.replace(',', "");
    } else {
        for sep in &fmt.grouping_separators {
            digits = digits.replace(sep.character, "");
        }
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

fn parse_integer_format_value(fmt: &IntegerFormat, captured: &str) -> Result<f64, DateTimeError> {
    match fmt.primary {
        PrimaryFormat::Letters => Ok(letters_to_decimal(
            captured,
            if fmt.case == TCase::Upper { 'A' } else { 'a' },
        ) as f64),
        PrimaryFormat::Roman => {
            let upper = if fmt.case == TCase::Upper {
                captured.to_string()
            } else {
                captured.to_uppercase()
            };
            Ok(roman_to_decimal(&upper) as f64)
        }
        PrimaryFormat::Words => words_to_number(&captured.to_lowercase())
            .ok_or_else(|| coded("D3136", "unrecognized number word")),
        PrimaryFormat::Decimal => decimal_digits(fmt, captured)
            .parse::<f64>()
            .map_err(|_| coded("D3136", "invalid number")),
        PrimaryFormat::Sequence => Err(coded("D3130", fmt.token.clone().unwrap_or_default())),
    }
}

fn name_lookup(
    component: char,
    captured: &str,
    width_max: Option<usize>,
) -> Result<i64, DateTimeError> {
    match component {
        'M' | 'x' => {
            for (i, name) in MONTHS.iter().enumerate() {
                let key: String = match width_max {
                    Some(w) => name.chars().take(w).collect(),
                    None => name.to_string(),
                };
                if key == captured {
                    return Ok((i + 1) as i64);
                }
            }
            Err(coded(
                "D3136",
                format!("unrecognized month name: {captured}"),
            ))
        }
        'F' => {
            for (i, name) in DAYS.iter().enumerate().skip(1) {
                let key: String = match width_max {
                    Some(w) => name.chars().take(w).collect(),
                    None => name.to_string(),
                };
                if key == captured {
                    return Ok(i as i64);
                }
            }
            Err(coded("D3136", format!("unrecognized day name: {captured}")))
        }
        'P' => match captured {
            "am" | "AM" => Ok(0),
            "pm" | "PM" => Ok(1),
            _ => Err(coded("D3136", "unrecognized am/pm marker")),
        },
        other => Err(coded("D3133", other.to_string())),
    }
}

fn build_matcher_parts(spec: &[PictureItem]) -> Result<Vec<MatcherPart>, DateTimeError> {
    let mut parts = Vec::with_capacity(spec.len());
    for item in spec {
        match item {
            PictureItem::Literal(lit) => parts.push(MatcherPart {
                regex: regex::escape(lit),
                component: None,
                parse_kind: None,
            }),
            PictureItem::Marker(m) => {
                if m.component == 'Z' || m.component == 'z' {
                    let fmt = m.integer_format.as_ref().unwrap();
                    let separator = if fmt.regular {
                        Some(fmt.grouping_separators[0].character)
                    } else {
                        None
                    };
                    let mut regex = String::new();
                    if m.component == 'z' {
                        regex.push_str("GMT");
                    }
                    regex.push_str("[-+][0-9]+");
                    if let Some(sep) = separator {
                        regex.push_str(&format!("{}[0-9]+", regex::escape(&sep.to_string())));
                    }
                    parts.push(MatcherPart {
                        regex,
                        component: Some(m.component),
                        parse_kind: Some(ParseKind::Timezone {
                            has_gmt: m.component == 'z',
                            separator,
                        }),
                    });
                } else if m.component == 'f' {
                    parts.push(MatcherPart {
                        regex: "[0-9]+".to_string(),
                        component: Some('f'),
                        parse_kind: Some(ParseKind::Fraction),
                    });
                } else if let Some(fmt) = &m.integer_format {
                    let regex = integer_format_regex(fmt)?;
                    parts.push(MatcherPart {
                        regex,
                        component: Some(m.component),
                        parse_kind: Some(ParseKind::Integer(fmt.clone())),
                    });
                } else {
                    // A names-based component (month/day/am-pm). Anything else here
                    // means the presentation modifier requested names for a component
                    // that doesn't support them (e.g. `[YN]`).
                    if !"MxFP".contains(m.component) {
                        return Err(coded("D3133", m.component.to_string()));
                    }
                    parts.push(MatcherPart {
                        regex: "[a-zA-Z]+".to_string(),
                        component: Some(m.component),
                        parse_kind: Some(ParseKind::Name {
                            width_max: m.width.as_ref().and_then(|w| w.max),
                        }),
                    });
                }
            }
        }
    }
    Ok(parts)
}

fn parse_component_value(
    kind: &ParseKind,
    component: char,
    captured: &str,
) -> Result<i64, DateTimeError> {
    match kind {
        ParseKind::Timezone { has_gmt, separator } => {
            let v = if *has_gmt { &captured[3..] } else { captured };
            if let Some(sep) = separator {
                let idx = v
                    .find(*sep)
                    .ok_or_else(|| coded("D3136", "invalid timezone"))?;
                let hours: i64 = v[..idx]
                    .parse()
                    .map_err(|_| coded("D3136", "invalid timezone"))?;
                let minutes: i64 = v[idx + sep.len_utf8()..]
                    .parse()
                    .map_err(|_| coded("D3136", "invalid timezone"))?;
                timezone_minutes(hours, minutes)
            } else {
                let numdigits = v.len() - 1;
                if numdigits <= 2 {
                    let hours: i64 = v.parse().map_err(|_| coded("D3136", "invalid timezone"))?;
                    timezone_minutes(hours, 0)
                } else {
                    let hours: i64 = v[..3]
                        .parse()
                        .map_err(|_| coded("D3136", "invalid timezone"))?;
                    let minutes: i64 = v[3..].parse().unwrap_or(0);
                    timezone_minutes(hours, minutes)
                }
            }
        }
        ParseKind::Fraction => {
            let truncated = &captured[..captured.len().min(3)];
            let padded = format!("0.{truncated}");
            let f: f64 = padded.parse().unwrap_or(0.0);
            Ok((f * 1000.0).round() as i64)
        }
        ParseKind::Integer(fmt) => Ok(parse_integer_format_value(fmt, captured)? as i64),
        ParseKind::Name { width_max } => name_lookup(component, captured, *width_max),
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

    // Guard from jsonata-js 2.3.0 (`analyseDateTimePicture`'s `cannotMatch`):
    // a timestamp needs every character of each literal and at least one per
    // marker, and matching costs time and memory proportional to the number
    // of picture parts times the timestamp length. A timestamp that is too
    // short, or a match that would be too large, cannot match -- return
    // undefined instead of compiling a regex that can exhaust memory. The
    // picture is analysed first, so an invalid one is still an error.
    let max_length = timestamp.chars().count();
    let min_length: usize = spec
        .iter()
        .map(|item| match item {
            PictureItem::Literal(lit) => lit.chars().count(),
            PictureItem::Marker(_) => 1,
        })
        .sum();
    if min_length > max_length || spec.len().saturating_mul(max_length + 1) > MAX_MATCH_SIZE {
        return Ok(None);
    }

    let parts = build_matcher_parts(&spec)?;

    let pattern = format!(
        "^{}$",
        parts
            .iter()
            .map(|p| format!("({})", p.regex))
            .collect::<Vec<_>>()
            .join("")
    );
    let re = RegexBuilder::new(&pattern)
        .case_insensitive(true)
        .build()
        .map_err(|e| DateTimeError::ParseError(format!("invalid picture-derived regex: {e}")))?;

    let caps = match re.captures(timestamp) {
        Some(c) => c,
        None => return Ok(None),
    };

    let mut components: HashMap<char, i64> = HashMap::new();
    for (i, part) in parts.iter().enumerate() {
        if let Some(kind) = &part.parse_kind {
            let captured = caps.get(i + 1).map(|m| m.as_str()).unwrap_or("");
            let value = parse_component_value(kind, part.component.unwrap(), captured)?;
            components.insert(part.component.unwrap(), value);
        }
    }

    if components.is_empty() {
        return Ok(None);
    }

    // jsonata-js builds the masks with `shift(components[part])`, which is
    // truthy-tested: a component that parsed as 0 (hour 0, "am", day 0)
    // counts as not given when choosing the date and time types.
    let mask_of = |order: &str| -> u8 {
        let mut mask = 0u8;
        for ch in order.chars() {
            let given = components.get(&ch).is_some_and(|value| *value != 0);
            mask = (mask << 1) | given as u8;
        }
        mask
    };
    let is_type = |type_: u8, mask: u8| (mask & !type_) == 0 && (mask & type_) != 0;

    let dmask = mask_of("YXMxWwdD");
    let dm_a = 0b1010_0001u8;
    let dm_b = 0b1000_0010u8;
    let dm_c = 0b0101_0100u8;
    let dm_d = 0b0100_1000u8;
    let date_a = is_type(dm_a, dmask);
    let date_b = !date_a && is_type(dm_b, dmask);
    let date_c = is_type(dm_c, dmask);
    let date_d = !date_c && is_type(dm_d, dmask);

    // dateC ({Y, x, w, F}, ISO week-numbering year/month) and dateD ({X, W, F}, ISO week
    // date) are recognised but not supported for parsing (matches the JS reference's
    // explicit "not currently supported" TODOs) -- fail fast rather than running the
    // defaulting walk below, since both ultimately raise the same D3136 regardless.
    if date_c || date_d {
        return Err(coded(
            "D3136",
            "ISO week-numbering year/month parsing is not supported",
        ));
    }

    let tmask = mask_of("PHhmsf");
    let time_a = is_type(0b010111, tmask);
    let time_b = !time_a && is_type(0b101111, tmask);

    let date_comps = if date_b { "YD" } else { "YMD" };
    let time_comps = if time_b { "Phmsf" } else { "Hmsf" };
    let comps: String = format!("{date_comps}{time_comps}");

    let mut start_specified = false;
    let mut end_specified = false;
    for part in comps.chars() {
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
                    e.insert(if "MDd".contains(part) { 1 } else { 0 });
                    end_specified = true;
                } else {
                    e.insert(get_date_time_fragment(now, part));
                }
            }
        }
    }

    let year = components[&'Y'];

    let hour_val = if time_b {
        let h = components[&'h'];
        let hh = if h == 12 { 0 } else { h };
        if components.get(&'P').copied() == Some(1) {
            match hh.checked_add(12) {
                Some(hour) => hour,
                // An hour this large is NaN from `Date.UTC` in jsonata-js.
                None => return Ok(Some(f64::NAN)),
            }
        } else {
            hh
        }
    } else {
        components[&'H']
    };

    // jsonata-js builds the timestamp with `Date.UTC`, which carries a field
    // past its range into the next one: month 13 is January of the next year,
    // 30 February is 1 or 2 March, hour 24 is midnight of the next day. A
    // month of 0 is January.
    let mut month0 = match components.get(&'M') {
        Some(&m) if m > 0 => m - 1,
        _ => 0,
    };
    let mut day = components.get(&'D').copied().unwrap_or(1);
    if date_b {
        // As jsonata-js: the month and day `d - 1` days on from 1 January,
        // kept with the parsed year, so a day past the year's end lands in
        // the same year, not the next. If 1 January or the derived date is
        // past JavaScript's range, the date is invalid and the result NaN.
        let derived = match date_utc(year, 0, 1, 0, 0, 0, 0)
            .and_then(|first_jan| {
                let offset_millis = components[&'d']
                    .checked_sub(1)?
                    .checked_mul(MILLIS_PER_DAY)?;
                first_jan.checked_add(offset_millis)
            })
            .filter(|derived| (-MAX_DATE_MILLIS..=MAX_DATE_MILLIS).contains(derived))
        {
            Some(derived) => derived,
            None => return Ok(Some(f64::NAN)),
        };
        let (_, m, d) = CivilDate::from_millis(derived).ymd();
        month0 = m - 1;
        day = d;
    }
    // A date past JavaScript's range is NaN from `Date.UTC`, and jsonata-js
    // returns that NaN.
    let Some(millis) = date_utc(
        year,
        month0,
        day,
        hour_val,
        components[&'m'],
        components[&'s'],
        components.get(&'f').copied().unwrap_or(0),
    ) else {
        return Ok(Some(f64::NAN));
    };
    // `millis -= (components.Z || components.z) * 60 * 1000`, in JavaScript
    // numbers: a zero Z falls through to z, and nothing clips the result, so
    // an offset can move it past JavaScript's range.
    let offset = components
        .get(&'Z')
        .filter(|offset| **offset != 0)
        .or_else(|| components.get(&'z'))
        .copied()
        .unwrap_or(0);
    Ok(Some(millis as f64 - offset as f64 * 60.0 * 1000.0))
}

/// A timezone offset in minutes, if it adds up without overflowing.
fn timezone_minutes(hours: i64, minutes: i64) -> Result<i64, DateTimeError> {
    hours
        .checked_mul(60)
        .and_then(|h| h.checked_add(minutes))
        .ok_or_else(|| coded("D3136", "invalid timezone"))
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
                -5.999999998485235e21,
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
        assert_eq!(roman_to_decimal("MMXVIII"), 2018);
    }

    #[test]
    fn test_letters_round_trip() {
        assert_eq!(decimal_to_letters(23, 'a'), "w");
        assert_eq!(letters_to_decimal("w", 'a'), 23);
    }
}

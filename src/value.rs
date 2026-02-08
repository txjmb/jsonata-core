// JValue: Rc-wrapped value type for O(1) cloning
// Replaces serde_json::Value for internal use

use std::fmt;
use std::rc::Rc;

use indexmap::IndexMap;
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};

/// A JSON-like value with O(1) clone semantics via Rc-wrapping.
///
/// Standard JSON types (Array, Object, String) are wrapped in Rc for cheap cloning.
/// Internal types (Undefined, Lambda, Builtin, Regex) are first-class variants
/// instead of tagged objects with hash-map lookups.
#[derive(Clone, Debug)]
pub enum JValue {
    // Standard JSON types
    Null,
    Bool(bool),
    Number(f64),
    String(Rc<str>),
    Array(Rc<Vec<JValue>>),
    Object(Rc<IndexMap<String, JValue>>),

    // Internal types (previously tagged objects)
    Undefined,
    Lambda {
        lambda_id: Rc<str>,
        params: Rc<Vec<String>>,
        name: Option<Rc<str>>,
        signature: Option<Rc<str>>,
    },
    Builtin {
        name: Rc<str>,
    },
    Regex {
        pattern: Rc<str>,
        flags: Rc<str>,
    },
}

// ── Type checks ──────────────────────────────────────────────────────────────

impl JValue {
    #[inline]
    pub fn is_null(&self) -> bool {
        matches!(self, JValue::Null)
    }

    #[inline]
    pub fn is_undefined(&self) -> bool {
        matches!(self, JValue::Undefined)
    }

    #[inline]
    pub fn is_bool(&self) -> bool {
        matches!(self, JValue::Bool(_))
    }

    #[inline]
    pub fn is_number(&self) -> bool {
        matches!(self, JValue::Number(_))
    }

    #[inline]
    pub fn is_string(&self) -> bool {
        matches!(self, JValue::String(_))
    }

    #[inline]
    pub fn is_array(&self) -> bool {
        matches!(self, JValue::Array(_))
    }

    #[inline]
    pub fn is_object(&self) -> bool {
        matches!(self, JValue::Object(_))
    }

    #[inline]
    pub fn is_lambda(&self) -> bool {
        matches!(self, JValue::Lambda { .. })
    }

    #[inline]
    pub fn is_builtin(&self) -> bool {
        matches!(self, JValue::Builtin { .. })
    }

    #[inline]
    pub fn is_function(&self) -> bool {
        matches!(self, JValue::Lambda { .. } | JValue::Builtin { .. })
    }

    #[inline]
    pub fn is_regex(&self) -> bool {
        matches!(self, JValue::Regex { .. })
    }
}

// ── Extraction ───────────────────────────────────────────────────────────────

impl JValue {
    #[inline]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            JValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            JValue::Number(n) => {
                let f = *n;
                if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                    Some(f as i64)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    #[inline]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            JValue::String(s) => Some(s),
            _ => None,
        }
    }

    #[inline]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            JValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    #[inline]
    pub fn as_array(&self) -> Option<&Vec<JValue>> {
        match self {
            JValue::Array(arr) => Some(arr),
            _ => None,
        }
    }

    #[inline]
    pub fn as_object(&self) -> Option<&IndexMap<String, JValue>> {
        match self {
            JValue::Object(map) => Some(map),
            _ => None,
        }
    }

    /// Get a mutable reference to the inner Vec, cloning if shared (Rc::make_mut).
    #[inline]
    pub fn as_array_mut(&mut self) -> Option<&mut Vec<JValue>> {
        match self {
            JValue::Array(arr) => Some(Rc::make_mut(arr)),
            _ => None,
        }
    }

    /// Get a mutable reference to the inner IndexMap, cloning if shared (Rc::make_mut).
    #[inline]
    pub fn as_object_mut(&mut self) -> Option<&mut IndexMap<String, JValue>> {
        match self {
            JValue::Object(map) => Some(Rc::make_mut(map)),
            _ => None,
        }
    }

    /// Get a reference to the Rc<str> for the string variant.
    #[inline]
    pub fn as_rc_str(&self) -> Option<&Rc<str>> {
        match self {
            JValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Index into an object by key.
    #[inline]
    pub fn get(&self, key: &str) -> Option<&JValue> {
        match self {
            JValue::Object(map) => map.get(key),
            _ => None,
        }
    }

    /// Index into an array by position.
    #[inline]
    pub fn get_index(&self, index: usize) -> Option<&JValue> {
        match self {
            JValue::Array(arr) => arr.get(index),
            _ => None,
        }
    }
}

// ── Constructors ─────────────────────────────────────────────────────────────

impl JValue {
    #[inline]
    pub fn from_i64(n: i64) -> Self {
        JValue::Number(n as f64)
    }

    #[inline]
    pub fn from_f64(n: f64) -> Self {
        JValue::Number(n)
    }

    #[inline]
    pub fn string(s: impl Into<Rc<str>>) -> Self {
        JValue::String(s.into())
    }

    #[inline]
    pub fn array(v: Vec<JValue>) -> Self {
        JValue::Array(Rc::new(v))
    }

    #[inline]
    pub fn object(m: IndexMap<String, JValue>) -> Self {
        JValue::Object(Rc::new(m))
    }

    #[inline]
    pub fn lambda(
        lambda_id: impl Into<Rc<str>>,
        params: Vec<String>,
        name: Option<impl Into<Rc<str>>>,
        signature: Option<impl Into<Rc<str>>>,
    ) -> Self {
        JValue::Lambda {
            lambda_id: lambda_id.into(),
            params: Rc::new(params),
            name: name.map(|n| n.into()),
            signature: signature.map(|s| s.into()),
        }
    }

    #[inline]
    pub fn builtin(name: impl Into<Rc<str>>) -> Self {
        JValue::Builtin { name: name.into() }
    }

    #[inline]
    pub fn regex(pattern: impl Into<Rc<str>>, flags: impl Into<Rc<str>>) -> Self {
        JValue::Regex {
            pattern: pattern.into(),
            flags: flags.into(),
        }
    }
}

// ── From impls ───────────────────────────────────────────────────────────────

impl From<bool> for JValue {
    #[inline]
    fn from(b: bool) -> Self {
        JValue::Bool(b)
    }
}

impl From<i64> for JValue {
    #[inline]
    fn from(n: i64) -> Self {
        JValue::Number(n as f64)
    }
}

impl From<i32> for JValue {
    #[inline]
    fn from(n: i32) -> Self {
        JValue::Number(n as f64)
    }
}

impl From<u64> for JValue {
    #[inline]
    fn from(n: u64) -> Self {
        JValue::Number(n as f64)
    }
}

impl From<usize> for JValue {
    #[inline]
    fn from(n: usize) -> Self {
        JValue::Number(n as f64)
    }
}

impl From<f64> for JValue {
    #[inline]
    fn from(n: f64) -> Self {
        JValue::Number(n)
    }
}

impl From<&str> for JValue {
    #[inline]
    fn from(s: &str) -> Self {
        JValue::String(s.into())
    }
}

impl From<String> for JValue {
    #[inline]
    fn from(s: String) -> Self {
        JValue::String(s.into())
    }
}

impl From<Rc<str>> for JValue {
    #[inline]
    fn from(s: Rc<str>) -> Self {
        JValue::String(s)
    }
}

impl From<Vec<JValue>> for JValue {
    #[inline]
    fn from(v: Vec<JValue>) -> Self {
        JValue::Array(Rc::new(v))
    }
}

impl From<IndexMap<String, JValue>> for JValue {
    #[inline]
    fn from(m: IndexMap<String, JValue>) -> Self {
        JValue::Object(Rc::new(m))
    }
}

// ── PartialEq ────────────────────────────────────────────────────────────────

impl PartialEq for JValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (JValue::Null, JValue::Null) => true,
            (JValue::Undefined, JValue::Undefined) => true,
            (JValue::Bool(a), JValue::Bool(b)) => a == b,
            (JValue::Number(a), JValue::Number(b)) => {
                // Handle NaN: NaN != NaN
                if a.is_nan() && b.is_nan() {
                    return false;
                }
                a == b
            }
            (JValue::String(a), JValue::String(b)) => a == b,
            (JValue::Array(a), JValue::Array(b)) => a == b,
            (JValue::Object(a), JValue::Object(b)) => a == b,
            (JValue::Lambda { lambda_id: a, .. }, JValue::Lambda { lambda_id: b, .. }) => a == b,
            (JValue::Builtin { name: a }, JValue::Builtin { name: b }) => a == b,
            (
                JValue::Regex {
                    pattern: ap,
                    flags: af,
                },
                JValue::Regex {
                    pattern: bp,
                    flags: bf,
                },
            ) => ap == bp && af == bf,
            _ => false,
        }
    }
}

// ── Display ──────────────────────────────────────────────────────────────────

impl fmt::Display for JValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JValue::Null => write!(f, "null"),
            JValue::Undefined => write!(f, "undefined"),
            JValue::Bool(b) => write!(f, "{}", b),
            JValue::Number(n) => format_number(*n, f),
            JValue::String(s) => write!(f, "\"{}\"", escape_json_string(s)),
            JValue::Array(arr) => {
                write!(f, "[")?;
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            JValue::Object(map) => {
                write!(f, "{{")?;
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "\"{}\":{}", escape_json_string(k), v)?;
                }
                write!(f, "}}")
            }
            JValue::Lambda { lambda_id, .. } => write!(f, "\"<lambda:{}>\"", lambda_id),
            JValue::Builtin { name } => write!(f, "\"<builtin:{}>\"", name),
            JValue::Regex { pattern, flags } => write!(f, "\"<regex:/{}/{}>\"", pattern, flags),
        }
    }
}

fn escape_json_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c < '\x20' => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    result
}

fn format_number(n: f64, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if !n.is_finite() {
        // NaN and +/-Infinity serialize as null (matching JSON spec)
        write!(f, "null")
    } else if n.fract() == 0.0 && n.abs() < 1e20 {
        write!(f, "{}", n as i64)
    } else {
        // Use serde_json's number formatting for consistency
        write!(f, "{}", n)
    }
}

// ── Serialization (for JSON output via evaluate_json) ────────────────────────

impl Serialize for JValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            JValue::Null => serializer.serialize_none(),
            JValue::Undefined => serializer.serialize_none(),
            JValue::Bool(b) => serializer.serialize_bool(*b),
            JValue::Number(n) => {
                if n.is_nan() || n.is_infinite() {
                    serializer.serialize_none()
                } else if n.fract() == 0.0 && *n >= i64::MIN as f64 && *n <= i64::MAX as f64 {
                    serializer.serialize_i64(*n as i64)
                } else {
                    serializer.serialize_f64(*n)
                }
            }
            JValue::String(s) => serializer.serialize_str(s),
            JValue::Array(arr) => {
                let mut seq = serializer.serialize_seq(Some(arr.len()))?;
                for v in arr.iter() {
                    seq.serialize_element(v)?;
                }
                seq.end()
            }
            JValue::Object(map) => {
                let mut m = serializer.serialize_map(Some(map.len()))?;
                for (k, v) in map.iter() {
                    m.serialize_entry(k, v)?;
                }
                m.end()
            }
            JValue::Lambda { .. } => serializer.serialize_str(""),
            JValue::Builtin { .. } => serializer.serialize_str(""),
            JValue::Regex { pattern, flags } => {
                let mut m = serializer.serialize_map(Some(2))?;
                m.serialize_entry("pattern", &**pattern)?;
                m.serialize_entry("flags", &**flags)?;
                m.end()
            }
        }
    }
}

// ── Deserialization (single-pass JSON→JValue) ────────────────────────────────

impl<'de> serde::Deserialize<'de> for JValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(JValueVisitor)
    }
}

struct JValueVisitor;

impl<'de> Visitor<'de> for JValueVisitor {
    type Value = JValue;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "any valid JSON value")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<JValue, E> {
        Ok(JValue::Bool(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<JValue, E> {
        Ok(JValue::Number(v as f64))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<JValue, E> {
        Ok(JValue::Number(v as f64))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<JValue, E> {
        Ok(JValue::Number(v))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<JValue, E> {
        Ok(JValue::string(v))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<JValue, E> {
        Ok(JValue::String(v.into()))
    }

    fn visit_none<E: de::Error>(self) -> Result<JValue, E> {
        Ok(JValue::Null)
    }

    fn visit_unit<E: de::Error>(self) -> Result<JValue, E> {
        Ok(JValue::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<JValue, A::Error> {
        let mut vec = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(elem) = seq.next_element()? {
            vec.push(elem);
        }
        Ok(JValue::array(vec))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<JValue, A::Error> {
        let mut m = IndexMap::with_capacity(map.size_hint().unwrap_or(0));
        while let Some((k, v)) = map.next_entry()? {
            m.insert(k, v);
        }
        Ok(JValue::object(m))
    }
}

// ── JSON string I/O ──────────────────────────────────────────────────────────

impl JValue {
    /// Serialize to a JSON string.
    pub fn to_json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Serialize to a pretty-printed JSON string.
    pub fn to_json_string_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Parse a JSON string into a JValue (single-pass, no intermediate serde_json::Value).
    ///
    /// When the `simd` feature is enabled, uses simd-json for 2-4x faster parsing
    /// on CPUs with SIMD support (SSE4.2/AVX2/NEON). Falls back to serde_json otherwise.
    pub fn from_json_str(s: &str) -> Result<JValue, serde_json::Error> {
        #[cfg(feature = "simd")]
        {
            // simd-json requires a mutable byte slice with padding
            let mut bytes = s.as_bytes().to_vec();
            if let Ok(value) = simd_json::serde::from_slice::<JValue>(&mut bytes) {
                return Ok(value);
            }
            // Fall back to serde_json if simd-json fails (e.g., unsupported CPU)
        }
        serde_json::from_str(s)
    }
}

// ── Conversion from serde_json::Value ────────────────────────────────────────

impl From<serde_json::Value> for JValue {
    fn from(v: serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => JValue::Null,
            serde_json::Value::Bool(b) => JValue::Bool(b),
            serde_json::Value::Number(n) => JValue::Number(n.as_f64().unwrap_or(0.0)),
            serde_json::Value::String(s) => JValue::String(s.into()),
            serde_json::Value::Array(arr) => {
                JValue::Array(Rc::new(arr.into_iter().map(JValue::from).collect()))
            }
            serde_json::Value::Object(map) => {
                let m: IndexMap<String, JValue> =
                    map.into_iter().map(|(k, v)| (k, JValue::from(v))).collect();
                JValue::Object(Rc::new(m))
            }
        }
    }
}

// ── Conversion to serde_json::Value (for Python boundary) ────────────────────

impl From<&JValue> for serde_json::Value {
    fn from(v: &JValue) -> Self {
        match v {
            JValue::Null | JValue::Undefined => serde_json::Value::Null,
            JValue::Bool(b) => serde_json::Value::Bool(*b),
            JValue::Number(n) => {
                if n.is_nan() || n.is_infinite() {
                    serde_json::Value::Null
                } else {
                    serde_json::json!(*n)
                }
            }
            JValue::String(s) => serde_json::Value::String(s.to_string()),
            JValue::Array(arr) => {
                serde_json::Value::Array(arr.iter().map(serde_json::Value::from).collect())
            }
            JValue::Object(map) => {
                let m: serde_json::Map<String, serde_json::Value> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::Value::from(v)))
                    .collect();
                serde_json::Value::Object(m)
            }
            JValue::Lambda { .. } | JValue::Builtin { .. } => serde_json::Value::Null,
            JValue::Regex { pattern, flags } => {
                let mut m = serde_json::Map::new();
                m.insert(
                    "pattern".to_string(),
                    serde_json::Value::String(pattern.to_string()),
                );
                m.insert(
                    "flags".to_string(),
                    serde_json::Value::String(flags.to_string()),
                );
                serde_json::Value::Object(m)
            }
        }
    }
}

// ── jvalue! macro ────────────────────────────────────────────────────────────

/// Macro for constructing JValue literals, similar to serde_json::json!
///
/// Usage:
///   jvalue!(null)           → JValue::Null
///   jvalue!(true)           → JValue::Bool(true)
///   jvalue!(false)          → JValue::Bool(false)
///   jvalue!(42)             → JValue::Number(42.0)
///   jvalue!(3.14)           → JValue::Number(3.14)
///   jvalue!("hello")        → JValue::String(Rc::from("hello"))
///   jvalue!([1, 2, 3])      → JValue::Array(Rc::new(vec![...]))
///   jvalue!({"k": v, ...})  → JValue::Object(Rc::new(IndexMap from pairs))
///   jvalue!(expr)           → JValue::from(expr)
#[macro_export]
macro_rules! jvalue {
    // null
    (null) => {
        $crate::value::JValue::Null
    };

    // true
    (true) => {
        $crate::value::JValue::Bool(true)
    };

    // false
    (false) => {
        $crate::value::JValue::Bool(false)
    };

    // Array
    ([ $($elem:tt),* $(,)? ]) => {
        $crate::value::JValue::Array(std::rc::Rc::new(vec![ $( jvalue!($elem) ),* ]))
    };

    // Object
    ({ $($key:tt : $val:tt),* $(,)? }) => {
        {
            let mut map = indexmap::IndexMap::new();
            $(
                map.insert(($key).to_string(), jvalue!($val));
            )*
            $crate::value::JValue::Object(std::rc::Rc::new(map))
        }
    };

    // Expression (fallback — numbers, variables, function calls, etc.)
    ($other:expr) => {
        $crate::value::JValue::from($other)
    };
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clone_is_cheap() {
        // Array clone should be O(1) — same Rc pointer
        let arr = JValue::array(vec![
            JValue::from(1i64),
            JValue::from(2i64),
            JValue::from(3i64),
        ]);
        let arr2 = arr.clone();
        if let (JValue::Array(a), JValue::Array(b)) = (&arr, &arr2) {
            assert!(Rc::ptr_eq(a, b));
        } else {
            panic!("expected arrays");
        }

        // Object clone should be O(1)
        let mut map = IndexMap::new();
        map.insert("x".to_string(), JValue::from(1i64));
        let obj = JValue::object(map);
        let obj2 = obj.clone();
        if let (JValue::Object(a), JValue::Object(b)) = (&obj, &obj2) {
            assert!(Rc::ptr_eq(a, b));
        } else {
            panic!("expected objects");
        }

        // String clone should be O(1)
        let s = JValue::string("hello");
        let s2 = s.clone();
        if let (JValue::String(a), JValue::String(b)) = (&s, &s2) {
            assert!(Rc::ptr_eq(a, b));
        } else {
            panic!("expected strings");
        }
    }

    #[test]
    fn test_type_checks() {
        assert!(JValue::Null.is_null());
        assert!(JValue::Undefined.is_undefined());
        assert!(JValue::Bool(true).is_bool());
        assert!(JValue::Number(42.0).is_number());
        assert!(JValue::string("hello").is_string());
        assert!(JValue::array(vec![]).is_array());
        assert!(JValue::object(IndexMap::new()).is_object());
        assert!(JValue::lambda("id", vec![], None::<&str>, None::<&str>).is_lambda());
        assert!(JValue::lambda("id", vec![], None::<&str>, None::<&str>).is_function());
        assert!(JValue::builtin("sum").is_builtin());
        assert!(JValue::builtin("sum").is_function());
        assert!(JValue::regex(".*", "i").is_regex());
    }

    #[test]
    fn test_extraction() {
        assert_eq!(JValue::Number(42.0).as_f64(), Some(42.0));
        assert_eq!(JValue::Number(42.0).as_i64(), Some(42));
        assert_eq!(JValue::Number(42.5).as_i64(), None);
        assert_eq!(JValue::string("hello").as_str(), Some("hello"));
        assert_eq!(JValue::Bool(true).as_bool(), Some(true));
        assert_eq!(
            JValue::array(vec![JValue::from(1i64)])
                .as_array()
                .map(|a| a.len()),
            Some(1)
        );
    }

    #[test]
    fn test_jvalue_macro() {
        let n = jvalue!(null);
        assert!(n.is_null());

        let b = jvalue!(true);
        assert_eq!(b.as_bool(), Some(true));

        let arr = jvalue!([1i64, 2i64, 3i64]);
        assert_eq!(arr.as_array().map(|a| a.len()), Some(3));

        let obj = jvalue!({"name": "Alice", "age": 30i64});
        assert_eq!(obj.get("name").and_then(|v| v.as_str()), Some("Alice"));
    }

    #[test]
    fn test_equality() {
        assert_eq!(JValue::Null, JValue::Null);
        assert_eq!(JValue::Bool(true), JValue::Bool(true));
        assert_ne!(JValue::Bool(true), JValue::Bool(false));
        assert_eq!(JValue::Number(42.0), JValue::Number(42.0));
        assert_ne!(JValue::Number(f64::NAN), JValue::Number(f64::NAN));
        assert_eq!(JValue::string("hello"), JValue::string("hello"));
        assert_ne!(JValue::Null, JValue::Undefined);
    }

    #[test]
    fn test_serde_roundtrip() {
        let v = jvalue!({"name": "Alice", "scores": [1i64, 2i64, 3i64], "active": true});
        let json_str = v.to_json_string().unwrap();
        let parsed = JValue::from_json_str(&json_str).unwrap();
        assert_eq!(v, parsed);
    }

    #[test]
    fn test_from_serde_json() {
        let sv = serde_json::json!({"name": "Alice", "age": 30, "scores": [1, 2, 3]});
        let jv = JValue::from(sv);
        assert_eq!(jv.get("name").and_then(|v| v.as_str()), Some("Alice"));
        assert_eq!(jv.get("age").and_then(|v| v.as_f64()), Some(30.0));
    }

    #[test]
    fn test_make_mut() {
        let mut arr = JValue::array(vec![JValue::from(1i64), JValue::from(2i64)]);
        let arr2 = arr.clone();

        // Mutate arr — should CoW (clone-on-write)
        arr.as_array_mut().unwrap().push(JValue::from(3i64));

        // arr2 should be unchanged
        assert_eq!(arr.as_array().unwrap().len(), 3);
        assert_eq!(arr2.as_array().unwrap().len(), 2);
    }
}

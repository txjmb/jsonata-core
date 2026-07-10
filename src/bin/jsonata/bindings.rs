use jsonata_core::value::JValue;
use std::collections::HashMap;

/// Parses `--arg NAME=VALUE` (bound as a string) and `--argjson NAME=JSON`
/// (bound as a parsed JSON value) specs into a name -> JValue map.
pub fn parse_bindings(
    arg: &[String],
    argjson: &[String],
) -> Result<HashMap<String, JValue>, String> {
    let mut bindings = HashMap::new();
    for spec in arg {
        let (name, value) = split_name_value(spec, "--arg")?;
        bindings.insert(name, JValue::string(value));
    }
    for spec in argjson {
        let (name, value) = split_name_value(spec, "--argjson")?;
        let parsed = JValue::from_json_str(&value)
            .map_err(|e| format!("--argjson {}: invalid JSON value: {}", name, e))?;
        bindings.insert(name, parsed);
    }
    Ok(bindings)
}

fn split_name_value(spec: &str, flag: &str) -> Result<(String, String), String> {
    match spec.split_once('=') {
        Some((name, value)) if !name.is_empty() => Ok((name.to_string(), value.to_string())),
        _ => Err(format!("{} expects NAME=VALUE, got: {}", flag, spec)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arg_binds_a_string() {
        let b = parse_bindings(&["region=us".to_string()], &[]).unwrap();
        assert_eq!(b.get("region"), Some(&JValue::string("us")));
    }

    #[test]
    fn argjson_binds_a_parsed_value() {
        let b = parse_bindings(&[], &["limit=42".to_string()]).unwrap();
        assert_eq!(b.get("limit"), Some(&JValue::Number(42.0)));
    }

    #[test]
    fn arg_without_equals_is_an_error() {
        assert!(parse_bindings(&["justaname".to_string()], &[]).is_err());
    }

    #[test]
    fn argjson_with_invalid_json_is_an_error() {
        assert!(parse_bindings(&[], &["x=not json".to_string()]).is_err());
    }

    #[test]
    fn arg_value_may_contain_equals_signs() {
        let b = parse_bindings(&["eq=a=b".to_string()], &[]).unwrap();
        assert_eq!(b.get("eq"), Some(&JValue::string("a=b")));
    }
}

//! Serde helpers for MCP JSON transport coercion.
//!
//! MCP clients sometimes serialize integer values as strings (e.g. `"0"` instead
//! of `0`). These helpers accept both native JSON numbers and string-encoded
//! numbers for seamless interoperability.

/// Deserialize an `i64` from either a JSON number or a string.
pub mod string_or_i64 {
    use serde::{Deserialize, Deserializer};
    use serde_json::Value;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<i64, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Value::deserialize(deserializer)? {
            Value::Number(n) => n
                .as_i64()
                .ok_or_else(|| serde::de::Error::custom("number out of i64 range")),
            Value::String(s) => s.parse::<i64>().map_err(serde::de::Error::custom),
            _ => Err(serde::de::Error::custom("expected number or string")),
        }
    }
}

/// Deserialize an `Option<i64>` from either a JSON number, a string, or null.
pub mod string_or_opt_i64 {
    use serde::{Deserialize, Deserializer};
    use serde_json::Value;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Value::deserialize(deserializer)? {
            Value::Null => Ok(None),
            Value::Number(n) => n
                .as_i64()
                .map(Some)
                .ok_or_else(|| serde::de::Error::custom("number out of i64 range")),
            Value::String(s) => s.parse::<i64>().map(Some).map_err(serde::de::Error::custom),
            _ => Err(serde::de::Error::custom("expected number, string, or null")),
        }
    }
}

/// Deserialize a `u64` from either a JSON number or a string.
pub mod string_or_u64 {
    use serde::{Deserialize, Deserializer};
    use serde_json::Value;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Value::deserialize(deserializer)? {
            Value::Number(n) => n
                .as_u64()
                .ok_or_else(|| serde::de::Error::custom("number out of u64 range")),
            Value::String(s) => s.parse::<u64>().map_err(serde::de::Error::custom),
            _ => Err(serde::de::Error::custom("expected number or string")),
        }
    }
}

/// Deserialize an `Option<u64>` from either a JSON number, a string, or null.
pub mod string_or_opt_u64 {
    use serde::{Deserialize, Deserializer};
    use serde_json::Value;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Value::deserialize(deserializer)? {
            Value::Null => Ok(None),
            Value::Number(n) => n
                .as_u64()
                .map(Some)
                .ok_or_else(|| serde::de::Error::custom("number out of u64 range")),
            Value::String(s) => s.parse::<u64>().map(Some).map_err(serde::de::Error::custom),
            _ => Err(serde::de::Error::custom("expected number, string, or null")),
        }
    }
}

/// Deserialize a `usize` from either a JSON number or a string.
pub mod string_or_usize {
    use serde::{Deserialize, Deserializer};
    use serde_json::Value;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<usize, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Value::deserialize(deserializer)? {
            Value::Number(n) => {
                let val = n
                    .as_u64()
                    .ok_or_else(|| serde::de::Error::custom("number out of usize range"))?;
                usize::try_from(val).map_err(serde::de::Error::custom)
            }
            Value::String(s) => s.parse::<usize>().map_err(serde::de::Error::custom),
            _ => Err(serde::de::Error::custom("expected number or string")),
        }
    }
}

/// Deserialize an `Option<usize>` from either a JSON number, a string, or null.
pub mod string_or_opt_usize {
    use serde::{Deserialize, Deserializer};
    use serde_json::Value;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<usize>, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Value::deserialize(deserializer)? {
            Value::Null => Ok(None),
            Value::Number(n) => {
                let val = n
                    .as_u64()
                    .ok_or_else(|| serde::de::Error::custom("number out of usize range"))?;
                usize::try_from(val)
                    .map(Some)
                    .map_err(serde::de::Error::custom)
            }
            Value::String(s) => s
                .parse::<usize>()
                .map(Some)
                .map_err(serde::de::Error::custom),
            _ => Err(serde::de::Error::custom("expected number, string, or null")),
        }
    }
}

/// Deserialize an `f64` from either a JSON number or a string.
pub mod string_or_f64 {
    use serde::{Deserialize, Deserializer};
    use serde_json::Value;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<f64, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Value::deserialize(deserializer)? {
            Value::Number(n) => n
                .as_f64()
                .ok_or_else(|| serde::de::Error::custom("invalid number")),
            Value::String(s) => s.parse::<f64>().map_err(serde::de::Error::custom),
            _ => Err(serde::de::Error::custom("expected number or string")),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Deserialize)]
    struct TestI64 {
        #[serde(deserialize_with = "super::string_or_i64::deserialize")]
        val: i64,
    }

    #[derive(Deserialize)]
    struct TestOptI64 {
        #[serde(default, deserialize_with = "super::string_or_opt_i64::deserialize")]
        val: Option<i64>,
    }

    #[derive(Deserialize)]
    struct TestU64 {
        #[serde(deserialize_with = "super::string_or_u64::deserialize")]
        val: u64,
    }

    #[derive(Deserialize)]
    struct TestOptU64 {
        #[serde(default, deserialize_with = "super::string_or_opt_u64::deserialize")]
        val: Option<u64>,
    }

    #[derive(Deserialize)]
    struct TestUsize {
        #[serde(deserialize_with = "super::string_or_usize::deserialize")]
        val: usize,
    }

    #[derive(Deserialize)]
    struct TestOptUsize {
        #[serde(default, deserialize_with = "super::string_or_opt_usize::deserialize")]
        val: Option<usize>,
    }

    #[derive(Deserialize)]
    struct TestF64 {
        #[serde(deserialize_with = "super::string_or_f64::deserialize")]
        val: f64,
    }

    #[test]
    fn i64_from_number() {
        let t: TestI64 = serde_json::from_value(json!({"val": 42})).unwrap();
        assert_eq!(t.val, 42);
    }

    #[test]
    fn i64_from_string() {
        let t: TestI64 = serde_json::from_value(json!({"val": "42"})).unwrap();
        assert_eq!(t.val, 42);
    }

    #[test]
    fn i64_negative() {
        let t: TestI64 = serde_json::from_value(json!({"val": "-1"})).unwrap();
        assert_eq!(t.val, -1);
        let t: TestI64 = serde_json::from_value(json!({"val": -1})).unwrap();
        assert_eq!(t.val, -1);
    }

    #[test]
    fn opt_i64_null() {
        let t: TestOptI64 = serde_json::from_value(json!({"val": null})).unwrap();
        assert_eq!(t.val, None);
    }

    #[test]
    fn opt_i64_missing() {
        let t: TestOptI64 = serde_json::from_value(json!({})).unwrap();
        assert_eq!(t.val, None);
    }

    #[test]
    fn opt_i64_present_number() {
        let t: TestOptI64 = serde_json::from_value(json!({"val": 10})).unwrap();
        assert_eq!(t.val, Some(10));
    }

    #[test]
    fn opt_i64_present_string() {
        let t: TestOptI64 = serde_json::from_value(json!({"val": "10"})).unwrap();
        assert_eq!(t.val, Some(10));
    }

    #[test]
    fn opt_i64_present_negative_string() {
        let t: TestOptI64 = serde_json::from_value(json!({"val": "-5"})).unwrap();
        assert_eq!(t.val, Some(-5));
    }

    #[test]
    fn u64_from_number() {
        let t: TestU64 = serde_json::from_value(json!({"val": 100})).unwrap();
        assert_eq!(t.val, 100);
    }

    #[test]
    fn u64_from_string() {
        let t: TestU64 = serde_json::from_value(json!({"val": "100"})).unwrap();
        assert_eq!(t.val, 100);
    }

    #[test]
    fn opt_u64_variants() {
        let t: TestOptU64 = serde_json::from_value(json!({})).unwrap();
        assert_eq!(t.val, None);
        let t: TestOptU64 = serde_json::from_value(json!({"val": null})).unwrap();
        assert_eq!(t.val, None);
        let t: TestOptU64 = serde_json::from_value(json!({"val": "300"})).unwrap();
        assert_eq!(t.val, Some(300));
    }

    #[test]
    fn usize_from_number() {
        let t: TestUsize = serde_json::from_value(json!({"val": 50})).unwrap();
        assert_eq!(t.val, 50);
    }

    #[test]
    fn usize_from_string() {
        let t: TestUsize = serde_json::from_value(json!({"val": "50"})).unwrap();
        assert_eq!(t.val, 50);
    }

    #[test]
    fn opt_usize_variants() {
        let t: TestOptUsize = serde_json::from_value(json!({})).unwrap();
        assert_eq!(t.val, None);
        let t: TestOptUsize = serde_json::from_value(json!({"val": "1000"})).unwrap();
        assert_eq!(t.val, Some(1000));
    }

    #[test]
    fn opt_usize_present_number() {
        let t: TestOptUsize = serde_json::from_value(json!({"val": 50})).unwrap();
        assert_eq!(t.val, Some(50));
    }

    #[test]
    fn f64_from_number() {
        let t: TestF64 = serde_json::from_value(json!({"val": 2.72})).unwrap();
        assert!((t.val - 2.72).abs() < f64::EPSILON);
    }

    #[test]
    fn f64_from_string() {
        let t: TestF64 = serde_json::from_value(json!({"val": "2.72"})).unwrap();
        assert!((t.val - 2.72).abs() < f64::EPSILON);
    }

    #[test]
    fn f64_from_integer_string() {
        let t: TestF64 = serde_json::from_value(json!({"val": "42"})).unwrap();
        assert!((t.val - 42.0).abs() < f64::EPSILON);
    }

    #[test]
    fn i64_rejects_invalid_string() {
        let result = serde_json::from_value::<TestI64>(json!({"val": "abc"}));
        assert!(result.is_err());
    }

    #[test]
    fn u64_rejects_negative() {
        let result = serde_json::from_value::<TestU64>(json!({"val": "-1"}));
        assert!(result.is_err());
    }
}

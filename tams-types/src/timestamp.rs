use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const NANOS_PER_SEC: i128 = 1_000_000_000;

/// Nanosecond-precision timestamp stored as a single i128.
///
/// String representation follows the TAMS spec: `"secs:nanos"` where
/// nanos is always 0-999999999 and seconds may be negative.
/// Regex: `^-?(0|[1-9][0-9]*):(0|[1-9][0-9]{0,8})$`
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp {
    pub nanos: i128,
}

impl Timestamp {
    pub fn secs(&self) -> i64 {
        self.nanos.div_euclid(NANOS_PER_SEC) as i64
    }

    pub fn subsec_nanos(&self) -> u32 {
        self.nanos.rem_euclid(NANOS_PER_SEC) as u32
    }
}

#[cfg(test)]
impl Timestamp {
    pub fn new(nanos: i128) -> Self {
        Self { nanos }
    }

    pub fn from_secs_nanos(secs: i64, nanos: u32) -> Self {
        Self {
            nanos: secs as i128 * NANOS_PER_SEC + nanos as i128,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimestampParseError(String);

impl fmt::Display for TimestampParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid timestamp: {}", self.0)
    }
}

impl std::error::Error for TimestampParseError {}

impl FromStr for Timestamp {
    type Err = TimestampParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let colon = s
            .find(':')
            .ok_or_else(|| TimestampParseError(format!("missing colon in '{}'", s)))?;
        let secs_str = &s[..colon];
        let nanos_str = &s[colon + 1..];

        // Validate seconds: optional '-', then either "0" or digit starting with 1-9
        let secs_abs = secs_str.strip_prefix('-').unwrap_or(secs_str);
        if secs_abs.is_empty() || !secs_abs.chars().all(|c| c.is_ascii_digit()) {
            return Err(TimestampParseError(format!(
                "invalid seconds: '{}'",
                secs_str
            )));
        }
        if secs_abs.len() > 1 && secs_abs.starts_with('0') {
            return Err(TimestampParseError(format!(
                "leading zeros in seconds: '{}'",
                secs_str
            )));
        }

        // Validate nanoseconds: either "0" or 1-9 digits starting with 1-9
        if nanos_str.is_empty() || !nanos_str.chars().all(|c| c.is_ascii_digit()) {
            return Err(TimestampParseError(format!(
                "invalid nanoseconds: '{}'",
                nanos_str
            )));
        }
        if nanos_str.len() > 1 && nanos_str.starts_with('0') {
            return Err(TimestampParseError(format!(
                "leading zeros in nanoseconds: '{}'",
                nanos_str
            )));
        }

        let secs: i128 = secs_str
            .parse()
            .map_err(|e| TimestampParseError(format!("seconds overflow '{}': {}", secs_str, e)))?;
        let nanos: i128 = nanos_str.parse().map_err(|e| {
            TimestampParseError(format!("nanoseconds overflow '{}': {}", nanos_str, e))
        })?;

        if nanos > 999_999_999 {
            return Err(TimestampParseError(format!(
                "nanoseconds out of range: {}",
                nanos
            )));
        }

        Ok(Timestamp {
            nanos: secs * NANOS_PER_SEC + nanos,
        })
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.secs(), self.subsec_nanos())
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <&str>::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Parsing --

    #[test]
    fn parse_zero() {
        let ts: Timestamp = "0:0".parse().unwrap();
        assert_eq!(ts.nanos, 0);
    }

    #[test]
    fn parse_positive() {
        let ts: Timestamp = "8:399999999".parse().unwrap();
        assert_eq!(ts.nanos, 8 * NANOS_PER_SEC + 399_999_999);
    }

    #[test]
    fn parse_negative() {
        let ts: Timestamp = "-1:500000000".parse().unwrap();
        assert_eq!(ts.nanos, -500_000_000);
    }

    #[test]
    fn parse_large_tai() {
        let ts: Timestamp = "1694429247:0".parse().unwrap();
        assert_eq!(ts.nanos, 1_694_429_247 * NANOS_PER_SEC);
    }

    #[test]
    fn parse_large_tai_with_nanos() {
        let ts: Timestamp = "1694429247:40000000".parse().unwrap();
        assert_eq!(ts.nanos, 1_694_429_247 * NANOS_PER_SEC + 40_000_000);
    }

    #[test]
    fn parse_negative_large() {
        let ts: Timestamp = "-100:0".parse().unwrap();
        assert_eq!(ts.nanos, -100 * NANOS_PER_SEC);
    }

    // -- Parse errors --

    #[test]
    fn parse_error_missing_colon() {
        assert!("12345".parse::<Timestamp>().is_err());
    }

    #[test]
    fn parse_error_leading_zero_seconds() {
        assert!("01:0".parse::<Timestamp>().is_err());
    }

    #[test]
    fn parse_error_leading_zero_nanos() {
        assert!("0:01".parse::<Timestamp>().is_err());
    }

    #[test]
    fn parse_error_nanos_too_large() {
        assert!("0:1000000000".parse::<Timestamp>().is_err());
    }

    #[test]
    fn parse_error_empty_parts() {
        assert!(":0".parse::<Timestamp>().is_err());
        assert!("0:".parse::<Timestamp>().is_err());
    }

    #[test]
    fn parse_error_letters() {
        assert!("abc:0".parse::<Timestamp>().is_err());
        assert!("0:abc".parse::<Timestamp>().is_err());
    }

    // -- Display --

    #[test]
    fn display_zero() {
        assert_eq!(Timestamp::new(0).to_string(), "0:0");
    }

    #[test]
    fn display_positive() {
        let ts = Timestamp::from_secs_nanos(8, 399999999);
        assert_eq!(ts.to_string(), "8:399999999");
    }

    #[test]
    fn display_negative() {
        let ts = Timestamp::new(-500_000_000);
        assert_eq!(ts.to_string(), "-1:500000000");
    }

    #[test]
    fn display_negative_exact_seconds() {
        let ts = Timestamp::new(-100 * NANOS_PER_SEC);
        assert_eq!(ts.to_string(), "-100:0");
    }

    // -- Round-trip --

    #[test]
    fn round_trip_parse_display() {
        let cases = [
            "0:0",
            "8:399999999",
            "-1:500000000",
            "1694429247:40000000",
            "-100:0",
        ];
        for case in cases {
            let ts: Timestamp = case.parse().unwrap();
            assert_eq!(ts.to_string(), case, "round-trip failed for '{}'", case);
        }
    }

    // -- Ordering --

    #[test]
    fn ordering_negative_lt_zero() {
        let neg: Timestamp = "-1:0".parse().unwrap();
        let zero: Timestamp = "0:0".parse().unwrap();
        assert!(neg < zero);
    }

    #[test]
    fn ordering_zero_lt_positive() {
        let zero: Timestamp = "0:0".parse().unwrap();
        let pos: Timestamp = "1:0".parse().unwrap();
        assert!(zero < pos);
    }

    #[test]
    fn ordering_subsecond() {
        let a: Timestamp = "0:100000000".parse().unwrap();
        let b: Timestamp = "0:200000000".parse().unwrap();
        assert!(a < b);
    }

    // -- Serde --

    #[test]
    fn serde_json_serializes_as_string() {
        let ts = Timestamp::new(0);
        let json = serde_json::to_string(&ts).unwrap();
        assert_eq!(json, "\"0:0\"");
    }

    #[test]
    fn serde_json_does_not_produce_object() {
        let ts = Timestamp::from_secs_nanos(10, 0);
        let json = serde_json::to_string(&ts).unwrap();
        assert!(
            !json.contains("nanos"),
            "should not serialize as object: {}",
            json
        );
        assert_eq!(json, "\"10:0\"");
    }

    #[test]
    fn serde_json_round_trip() {
        let ts = Timestamp::from_secs_nanos(1694429247, 40000000);
        let json = serde_json::to_string(&ts).unwrap();
        let ts2: Timestamp = serde_json::from_str(&json).unwrap();
        assert_eq!(ts, ts2);
    }

    #[test]
    fn serde_json_deserialize_from_string() {
        let ts: Timestamp = serde_json::from_str("\"8:399999999\"").unwrap();
        assert_eq!(ts.nanos, 8 * NANOS_PER_SEC + 399_999_999);
    }

    // -- Helper methods --

    #[test]
    fn secs_and_subsec_nanos() {
        let ts = Timestamp::from_secs_nanos(42, 123456789);
        assert_eq!(ts.secs(), 42);
        assert_eq!(ts.subsec_nanos(), 123456789);
    }

    #[test]
    fn secs_and_subsec_nanos_negative() {
        let ts: Timestamp = "-1:500000000".parse().unwrap();
        assert_eq!(ts.secs(), -1);
        assert_eq!(ts.subsec_nanos(), 500000000);
    }
}

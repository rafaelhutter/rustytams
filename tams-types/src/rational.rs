use serde::{Deserialize, Serialize};

fn default_denominator() -> i64 {
    1
}

/// Raw deserialization target -- validated via TryFrom before becoming Rational.
#[derive(Deserialize)]
struct RationalRaw {
    numerator: i64,
    #[serde(default = "default_denominator")]
    denominator: i64,
}

/// Rational number as numerator/denominator pair.
///
/// Used for frame rates (30000/1001 = 29.97fps), segment durations,
/// aspect ratios, and pixel aspect ratios. No floating point in the
/// timing pipeline.
///
/// The TAMS spec requires both numerator and denominator to be positive
/// (`exclusiveMinimum: 0`). This is enforced on deserialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RationalRaw")]
pub struct Rational {
    pub numerator: i64,
    pub denominator: i64,
}

impl TryFrom<RationalRaw> for Rational {
    type Error = String;

    fn try_from(raw: RationalRaw) -> Result<Self, String> {
        if raw.numerator <= 0 {
            return Err(format!("numerator must be positive, got {}", raw.numerator));
        }
        if raw.denominator <= 0 {
            return Err(format!(
                "denominator must be positive, got {}",
                raw.denominator
            ));
        }
        Ok(Rational {
            numerator: raw.numerator,
            denominator: raw.denominator,
        })
    }
}

impl Rational {
    pub fn new(numerator: i64, denominator: i64) -> Self {
        debug_assert!(numerator > 0, "numerator must be positive");
        debug_assert!(denominator > 0, "denominator must be positive");
        Self {
            numerator,
            denominator,
        }
    }

    /// Convert a rational duration (in seconds) to nanoseconds.
    ///
    /// Treats self as `numerator/denominator` seconds.
    /// Example: `Rational::new(10, 1).to_nanos()` = 10_000_000_000
    pub fn to_nanos(self) -> i64 {
        (1_000_000_000_i128 * self.numerator as i128 / self.denominator as i128) as i64
    }

    /// Duration of one frame in nanoseconds when self is a frame rate.
    ///
    /// Formula: `1_000_000_000 * denominator / numerator`
    /// Example: `Rational::new(30000, 1001).frame_duration_nanos()` = 33_366_666
    pub fn frame_duration_nanos(self) -> i64 {
        (1_000_000_000_i128 * self.denominator as i128 / self.numerator as i128) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Construction --

    #[test]
    fn new_basic() {
        let r = Rational::new(30000, 1001);
        assert_eq!(r.numerator, 30000);
        assert_eq!(r.denominator, 1001);
    }

    #[test]
    fn new_simple_rate() {
        let r = Rational::new(25, 1);
        assert_eq!(r.numerator, 25);
        assert_eq!(r.denominator, 1);
    }

    // -- Serde --

    #[test]
    fn serde_round_trip() {
        let r = Rational::new(30000, 1001);
        let json = serde_json::to_string(&r).unwrap();
        let r2: Rational = serde_json::from_str(&json).unwrap();
        assert_eq!(r, r2);
    }

    #[test]
    fn serde_format() {
        let r = Rational::new(30000, 1001);
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"numerator\":30000"));
        assert!(json.contains("\"denominator\":1001"));
    }

    #[test]
    fn serde_default_denominator() {
        let json = r#"{"numerator": 25}"#;
        let r: Rational = serde_json::from_str(json).unwrap();
        assert_eq!(r.numerator, 25);
        assert_eq!(r.denominator, 1);
    }

    #[test]
    fn serde_explicit_denominator_1() {
        let r = Rational::new(25, 1);
        let json = serde_json::to_string(&r).unwrap();
        let r2: Rational = serde_json::from_str(&json).unwrap();
        assert_eq!(r2.denominator, 1);
    }

    #[test]
    fn serde_round_trip_with_default() {
        let json_in = r#"{"numerator": 25}"#;
        let r: Rational = serde_json::from_str(json_in).unwrap();
        let json_out = serde_json::to_string(&r).unwrap();
        let r2: Rational = serde_json::from_str(&json_out).unwrap();
        assert_eq!(r, r2);
        assert_eq!(r2, Rational::new(25, 1));
    }

    // -- Deserialization validation --

    #[test]
    fn serde_rejects_zero_numerator() {
        let json = r#"{"numerator": 0, "denominator": 1}"#;
        let result: Result<Rational, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn serde_rejects_negative_numerator() {
        let json = r#"{"numerator": -5, "denominator": 1}"#;
        let result: Result<Rational, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn serde_rejects_zero_denominator() {
        let json = r#"{"numerator": 25, "denominator": 0}"#;
        let result: Result<Rational, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn serde_rejects_negative_denominator() {
        let json = r#"{"numerator": 25, "denominator": -1}"#;
        let result: Result<Rational, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    // -- Arithmetic --

    #[test]
    fn frame_duration_25fps() {
        let r = Rational::new(25, 1);
        assert_eq!(r.frame_duration_nanos(), 40_000_000);
    }

    #[test]
    fn frame_duration_29_97fps() {
        let r = Rational::new(30000, 1001);
        assert_eq!(r.frame_duration_nanos(), 33_366_666);
    }

    #[test]
    fn frame_duration_50fps() {
        let r = Rational::new(50, 1);
        assert_eq!(r.frame_duration_nanos(), 20_000_000);
    }

    #[test]
    fn to_nanos_10_seconds() {
        let r = Rational::new(10, 1);
        assert_eq!(r.to_nanos(), 10_000_000_000);
    }

    #[test]
    fn to_nanos_half_second() {
        let r = Rational::new(1, 2);
        assert_eq!(r.to_nanos(), 500_000_000);
    }

    #[test]
    fn to_nanos_ntsc_frame() {
        let r = Rational::new(1001, 30000);
        assert_eq!(r.to_nanos(), 33_366_666);
    }
}

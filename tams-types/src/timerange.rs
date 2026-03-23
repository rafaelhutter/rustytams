use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::timestamp::Timestamp;

/// One end of a time range: a timestamp with inclusive/exclusive flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bound {
    pub timestamp: Timestamp,
    pub inclusive: bool,
}

/// A time range following the TAMS spec notation.
///
/// Examples: `"_"` (eternity), `"()"` (never), `"[0:0_10:0)"`, `"[10:0]"` (instant).
/// Eternity is represented as `Range { start: None, end: None }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeRange {
    /// Empty range: contains no timestamps.
    Never,
    /// A range with optional start and end bounds.
    /// `None` means unbounded in that direction.
    Range {
        start: Option<Bound>,
        end: Option<Bound>,
    },
}

impl TimeRange {
    pub fn never() -> Self {
        TimeRange::Never
    }

    pub fn is_eternity(&self) -> bool {
        matches!(
            self,
            TimeRange::Range {
                start: None,
                end: None
            }
        )
    }

    pub fn is_never(&self) -> bool {
        matches!(self, TimeRange::Never)
    }

    /// Check if this range overlaps with another.
    pub fn overlaps(&self, other: &TimeRange) -> bool {
        match (self, other) {
            (TimeRange::Never, _) | (_, TimeRange::Never) => false,
            (TimeRange::Range { start: s1, end: e1 }, TimeRange::Range { start: s2, end: e2 }) => {
                start_before_end(s1, e2) && start_before_end(s2, e1)
            }
        }
    }

    /// True if this range completely covers `other` (every point in other is in self).
    pub fn covers(&self, other: &TimeRange) -> bool {
        self.intersect(other) == *other
    }

    /// Compute the bounding union of two ranges.
    pub fn union(&self, other: &TimeRange) -> TimeRange {
        match (self, other) {
            (TimeRange::Never, x) | (x, TimeRange::Never) => *x,
            (TimeRange::Range { start: s1, end: e1 }, TimeRange::Range { start: s2, end: e2 }) => {
                TimeRange::Range {
                    start: select_bound(s1, s2, true),
                    end: select_bound(e1, e2, false),
                }
            }
        }
    }

    /// Compute the intersection of two ranges.
    ///
    /// Takes the latest start and earliest end. Used for clipping
    /// a flow's timerange to a query timerange.
    pub fn intersect(&self, other: &TimeRange) -> TimeRange {
        match (self, other) {
            (TimeRange::Never, _) | (_, TimeRange::Never) => TimeRange::Never,
            (TimeRange::Range { start: s1, end: e1 }, TimeRange::Range { start: s2, end: e2 }) => {
                let start = intersect_bound_start(s1, s2);
                let end = intersect_bound_end(e1, e2);
                if !start_before_end(&start, &end) {
                    TimeRange::Never
                } else {
                    TimeRange::Range { start, end }
                }
            }
        }
    }
}

#[cfg(test)]
impl TimeRange {
    pub fn eternity() -> Self {
        TimeRange::Range {
            start: None,
            end: None,
        }
    }

    /// Check if this range contains a specific timestamp.
    pub fn contains(&self, ts: &Timestamp) -> bool {
        match self {
            TimeRange::Never => false,
            TimeRange::Range { start, end } => {
                let after_start = match start {
                    None => true,
                    Some(b) => {
                        if b.inclusive {
                            *ts >= b.timestamp
                        } else {
                            *ts > b.timestamp
                        }
                    }
                };
                let before_end = match end {
                    None => true,
                    Some(b) => {
                        if b.inclusive {
                            *ts <= b.timestamp
                        } else {
                            *ts < b.timestamp
                        }
                    }
                };
                after_start && before_end
            }
        }
    }
}

/// For intersection start: pick the LATER start.
/// None (unbounded = -infinity) loses to any bounded start.
fn intersect_bound_start(a: &Option<Bound>, b: &Option<Bound>) -> Option<Bound> {
    match (a, b) {
        (None, x) | (x, None) => *x,
        (Some(a), Some(b)) => match a.timestamp.cmp(&b.timestamp) {
            Ordering::Equal => Some(Bound {
                timestamp: a.timestamp,
                inclusive: a.inclusive && b.inclusive,
            }),
            Ordering::Less => Some(*b),
            Ordering::Greater => Some(*a),
        },
    }
}

/// For intersection end: pick the EARLIER end.
/// None (unbounded = +infinity) loses to any bounded end.
fn intersect_bound_end(a: &Option<Bound>, b: &Option<Bound>) -> Option<Bound> {
    match (a, b) {
        (None, x) | (x, None) => *x,
        (Some(a), Some(b)) => match a.timestamp.cmp(&b.timestamp) {
            Ordering::Equal => Some(Bound {
                timestamp: a.timestamp,
                inclusive: a.inclusive && b.inclusive,
            }),
            Ordering::Less => Some(*a),
            Ordering::Greater => Some(*b),
        },
    }
}

/// True if the start bound comes before the end bound (they can overlap).
fn start_before_end(start: &Option<Bound>, end: &Option<Bound>) -> bool {
    match (start, end) {
        (None, _) | (_, None) => true,
        (Some(s), Some(e)) => match s.timestamp.cmp(&e.timestamp) {
            Ordering::Less => true,
            Ordering::Greater => false,
            Ordering::Equal => s.inclusive && e.inclusive,
        },
    }
}

fn select_bound(a: &Option<Bound>, b: &Option<Bound>, pick_lesser: bool) -> Option<Bound> {
    match (a, b) {
        (None, _) | (_, None) => None,
        (Some(a), Some(b)) => match a.timestamp.cmp(&b.timestamp) {
            Ordering::Equal => Some(Bound {
                timestamp: a.timestamp,
                inclusive: a.inclusive || b.inclusive,
            }),
            Ordering::Less => Some(if pick_lesser { *a } else { *b }),
            Ordering::Greater => Some(if pick_lesser { *b } else { *a }),
        },
    }
}

// -- Parsing --

#[derive(Debug, Clone, PartialEq)]
pub struct TimeRangeParseError(String);

impl fmt::Display for TimeRangeParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid timerange: {}", self.0)
    }
}

impl std::error::Error for TimeRangeParseError {}

impl FromStr for TimeRange {
    type Err = TimeRangeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(TimeRangeParseError("empty string".to_string()));
        }

        // Bracket-only strings with no timestamps are degenerate ranges → Never.
        // The spec regex allows these (all optional groups can be empty).
        // Note: "_" is eternity (handled by the general parser), not Never.
        let only_brackets = s.bytes().all(|b| matches!(b, b'[' | b']' | b'(' | b')'));
        if only_brackets {
            return Ok(TimeRange::Never);
        }

        let bytes = s.as_bytes();
        let len = bytes.len();
        let mut pos = 0;

        // Parse optional start bracket
        let start_bracket = match bytes[0] {
            b'[' => {
                pos = 1;
                Some(true)
            }
            b'(' => {
                pos = 1;
                Some(false)
            }
            _ => None,
        };

        // Parse optional end bracket
        let end_bracket = match bytes[len - 1] {
            b']' => Some(true),
            b')' => Some(false),
            _ => None,
        };

        let content_end = if end_bracket.is_some() { len - 1 } else { len };
        let content = &s[pos..content_end];

        match content.find('_') {
            Some(sep) => {
                // Range with separator
                let start_str = &content[..sep];
                let end_str = &content[sep + 1..];

                let start = if start_str.is_empty() {
                    None
                } else {
                    let ts: Timestamp = start_str
                        .parse()
                        .map_err(|e| TimeRangeParseError(format!("start: {}", e)))?;
                    Some(Bound {
                        timestamp: ts,
                        inclusive: start_bracket.unwrap_or(true),
                    })
                };

                let end = if end_str.is_empty() {
                    None
                } else {
                    let ts: Timestamp = end_str
                        .parse()
                        .map_err(|e| TimeRangeParseError(format!("end: {}", e)))?;
                    Some(Bound {
                        timestamp: ts,
                        inclusive: end_bracket.unwrap_or(true),
                    })
                };

                // Degenerate: end before start, or equal with exclusive marker
                if let (Some(s), Some(e)) = (&start, &end) {
                    if s.timestamp > e.timestamp {
                        return Ok(TimeRange::Never);
                    }
                    if s.timestamp == e.timestamp && (!s.inclusive || !e.inclusive) {
                        return Ok(TimeRange::Never);
                    }
                }

                Ok(TimeRange::Range { start, end })
            }
            None => {
                // No separator: must be instantaneous like "[10:0]"
                if content.is_empty() {
                    return Err(TimeRangeParseError(
                        "empty content with no separator".to_string(),
                    ));
                }

                let ts: Timestamp = content
                    .parse()
                    .map_err(|e| TimeRangeParseError(format!("{}", e)))?;

                let s_inc = start_bracket.unwrap_or(true);
                let e_inc = end_bracket.unwrap_or(true);

                // Exclusive markers on instantaneous = empty
                if !s_inc || !e_inc {
                    return Ok(TimeRange::Never);
                }

                Ok(TimeRange::Range {
                    start: Some(Bound {
                        timestamp: ts,
                        inclusive: true,
                    }),
                    end: Some(Bound {
                        timestamp: ts,
                        inclusive: true,
                    }),
                })
            }
        }
    }
}

// -- Display --

impl fmt::Display for TimeRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeRange::Never => write!(f, "()"),
            TimeRange::Range { start, end } => match (start, end) {
                (None, None) => write!(f, "_"),
                (Some(s), None) => {
                    let bracket = if s.inclusive { '[' } else { '(' };
                    write!(f, "{}{}_", bracket, s.timestamp)
                }
                (None, Some(e)) => {
                    let bracket = if e.inclusive { ']' } else { ')' };
                    write!(f, "_{}{}", e.timestamp, bracket)
                }
                (Some(s), Some(e)) => {
                    // Instantaneous: same timestamp, both inclusive
                    if s.timestamp == e.timestamp && s.inclusive && e.inclusive {
                        write!(f, "[{}]", s.timestamp)
                    } else {
                        let sb = if s.inclusive { '[' } else { '(' };
                        let eb = if e.inclusive { ']' } else { ')' };
                        write!(f, "{}{}_{}{}", sb, s.timestamp, e.timestamp, eb)
                    }
                }
            },
        }
    }
}

// -- Serde: custom string-based --

impl Serialize for TimeRange {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for TimeRange {
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
    fn parse_eternity() {
        let tr: TimeRange = "_".parse().unwrap();
        assert!(tr.is_eternity());
    }

    #[test]
    fn parse_never() {
        let tr: TimeRange = "()".parse().unwrap();
        assert!(tr.is_never());
    }

    #[test]
    fn parse_inclusive_exclusive() {
        let tr: TimeRange = "[0:0_10:0)".parse().unwrap();
        match &tr {
            TimeRange::Range {
                start: Some(s),
                end: Some(e),
            } => {
                assert_eq!(s.timestamp, Timestamp::new(0));
                assert!(s.inclusive);
                assert_eq!(e.timestamp, Timestamp::new(10 * NANOS_PER_SEC));
                assert!(!e.inclusive);
            }
            _ => panic!("expected Range, got {:?}", tr),
        }
    }

    #[test]
    fn parse_exclusive_start_unbounded_end() {
        let tr: TimeRange = "(5:0_".parse().unwrap();
        match &tr {
            TimeRange::Range {
                start: Some(s),
                end: None,
            } => {
                assert_eq!(s.timestamp, Timestamp::new(5 * NANOS_PER_SEC));
                assert!(!s.inclusive);
            }
            _ => panic!("expected Range with unbounded end, got {:?}", tr),
        }
    }

    #[test]
    fn parse_unbounded_start_inclusive_end() {
        let tr: TimeRange = "_20:0]".parse().unwrap();
        match &tr {
            TimeRange::Range {
                start: None,
                end: Some(e),
            } => {
                assert_eq!(e.timestamp, Timestamp::new(20 * NANOS_PER_SEC));
                assert!(e.inclusive);
            }
            _ => panic!("expected Range with unbounded start, got {:?}", tr),
        }
    }

    #[test]
    fn parse_instantaneous() {
        let tr: TimeRange = "[10:0]".parse().unwrap();
        match &tr {
            TimeRange::Range {
                start: Some(s),
                end: Some(e),
            } => {
                assert_eq!(s.timestamp, e.timestamp);
                assert!(s.inclusive);
                assert!(e.inclusive);
            }
            _ => panic!("expected instantaneous Range, got {:?}", tr),
        }
    }

    #[test]
    fn parse_instantaneous_with_separator() {
        let tr: TimeRange = "[10:0_10:0]".parse().unwrap();
        match &tr {
            TimeRange::Range {
                start: Some(s),
                end: Some(e),
            } => {
                assert_eq!(s.timestamp, e.timestamp);
                assert!(s.inclusive);
                assert!(e.inclusive);
            }
            _ => panic!("expected instantaneous Range, got {:?}", tr),
        }
    }

    // -- Degenerate cases --

    #[test]
    fn parse_exclusive_instant_is_never() {
        let tr: TimeRange = "(10:0)".parse().unwrap();
        assert!(tr.is_never());
    }

    #[test]
    fn parse_end_before_start_is_never() {
        let tr: TimeRange = "[10:0_5:0]".parse().unwrap();
        assert!(tr.is_never());
    }

    #[test]
    fn parse_equal_exclusive_end_is_never() {
        let tr: TimeRange = "[10:0_10:0)".parse().unwrap();
        assert!(tr.is_never());
    }

    // -- Parse errors --

    #[test]
    fn parse_error_empty() {
        assert!("".parse::<TimeRange>().is_err());
    }

    #[test]
    fn parse_error_bad_timestamp() {
        assert!("[abc_10:0)".parse::<TimeRange>().is_err());
    }

    #[test]
    fn parse_bracket_only_as_never() {
        // All bracket-only combinations are valid per spec regex and parse as Never
        for s in ["[]", "[)", "(]", "()", "[", "]", "(", ")"] {
            assert_eq!(
                s.parse::<TimeRange>().unwrap(),
                TimeRange::Never,
                "{s} should parse as Never"
            );
        }
    }

    // -- Display --

    #[test]
    fn display_eternity() {
        assert_eq!(TimeRange::eternity().to_string(), "_");
    }

    #[test]
    fn display_never() {
        assert_eq!(TimeRange::never().to_string(), "()");
    }

    #[test]
    fn display_range() {
        let tr: TimeRange = "[0:0_10:0)".parse().unwrap();
        assert_eq!(tr.to_string(), "[0:0_10:0)");
    }

    #[test]
    fn display_instantaneous() {
        let tr: TimeRange = "[10:0]".parse().unwrap();
        assert_eq!(tr.to_string(), "[10:0]");
    }

    #[test]
    fn display_unbounded_start() {
        let tr: TimeRange = "_20:0]".parse().unwrap();
        assert_eq!(tr.to_string(), "_20:0]");
    }

    #[test]
    fn display_unbounded_end() {
        let tr: TimeRange = "(5:0_".parse().unwrap();
        assert_eq!(tr.to_string(), "(5:0_");
    }

    // -- Round-trip --

    #[test]
    fn round_trip_parse_display() {
        let cases = [
            "_",
            "()",
            "[0:0_10:0)",
            "(5:0_",
            "_20:0]",
            "[10:0]",
            "[100:0_200:0]",
            "(0:0_1:0)",
        ];
        for case in cases {
            let tr: TimeRange = case.parse().unwrap();
            assert_eq!(tr.to_string(), case, "round-trip failed for '{}'", case);
        }
    }

    // -- Overlap --

    use crate::timestamp::NANOS_PER_SEC;

    fn range(s: &str) -> TimeRange {
        s.parse().unwrap()
    }

    #[test]
    fn overlap_eternity_overlaps_everything() {
        assert!(range("_").overlaps(&range("[0:0_10:0)")));
        assert!(range("_").overlaps(&range("_")));
        assert!(range("[0:0_10:0)").overlaps(&range("_")));
    }

    #[test]
    fn overlap_never_overlaps_nothing() {
        assert!(!range("()").overlaps(&range("[0:0_10:0)")));
        assert!(!range("()").overlaps(&range("_")));
        assert!(!range("()").overlaps(&range("()")));
    }

    #[test]
    fn overlap_adjacent_no_overlap() {
        // [0:0_10:0) and [10:0_20:0) are adjacent, not overlapping
        assert!(!range("[0:0_10:0)").overlaps(&range("[10:0_20:0)")));
    }

    #[test]
    fn overlap_adjacent_inclusive_does_overlap() {
        // [0:0_10:0] and [10:0_20:0) overlap at 10:0
        assert!(range("[0:0_10:0]").overlaps(&range("[10:0_20:0)")));
    }

    #[test]
    fn overlap_partial() {
        assert!(range("[0:0_15:0)").overlaps(&range("[10:0_20:0)")));
    }

    #[test]
    fn overlap_contained() {
        assert!(range("[0:0_20:0)").overlaps(&range("[5:0_15:0)")));
    }

    #[test]
    fn overlap_instantaneous_in_range() {
        assert!(range("[5:0]").overlaps(&range("[0:0_10:0)")));
    }

    #[test]
    fn overlap_instantaneous_at_exclusive_boundary() {
        // [10:0] does NOT overlap [0:0_10:0) because 10:0 is excluded
        assert!(!range("[10:0]").overlaps(&range("[0:0_10:0)")));
    }

    // -- Contains --

    #[test]
    fn contains_point_in_range() {
        let tr = range("[0:0_10:0)");
        assert!(tr.contains(&Timestamp::new(5 * NANOS_PER_SEC)));
    }

    #[test]
    fn contains_start_inclusive() {
        let tr = range("[0:0_10:0)");
        assert!(tr.contains(&Timestamp::new(0)));
    }

    #[test]
    fn contains_end_exclusive() {
        let tr = range("[0:0_10:0)");
        assert!(!tr.contains(&Timestamp::new(10 * NANOS_PER_SEC)));
    }

    #[test]
    fn contains_eternity_contains_all() {
        let tr = range("_");
        assert!(tr.contains(&Timestamp::new(0)));
        assert!(tr.contains(&Timestamp::new(-1_000_000_000)));
        assert!(tr.contains(&Timestamp::new(999_999_999_999)));
    }

    #[test]
    fn contains_never_contains_nothing() {
        let tr = range("()");
        assert!(!tr.contains(&Timestamp::new(0)));
    }

    // -- Union --

    #[test]
    fn union_with_never() {
        let tr = range("[0:0_10:0)");
        assert_eq!(tr.union(&TimeRange::never()), tr);
        assert_eq!(TimeRange::never().union(&tr), tr);
    }

    #[test]
    fn union_extends_range() {
        let a = range("[0:0_10:0)");
        let b = range("[5:0_20:0)");
        let u = a.union(&b);
        // Should be [0:0_20:0)
        assert_eq!(u.to_string(), "[0:0_20:0)");
    }

    #[test]
    fn union_with_eternity() {
        let a = range("[0:0_10:0)");
        let u = a.union(&TimeRange::eternity());
        assert!(u.is_eternity());
    }

    // -- Intersect --

    #[test]
    fn intersect_overlapping() {
        let a = range("[0:0_20:0)");
        let b = range("[5:0_15:0)");
        let i = a.intersect(&b);
        assert_eq!(i.to_string(), "[5:0_15:0)");
    }

    #[test]
    fn intersect_contained() {
        let a = range("[0:0_20:0)");
        let b = range("[0:0_10:0)");
        let i = a.intersect(&b);
        assert_eq!(i.to_string(), "[0:0_10:0)");
    }

    #[test]
    fn intersect_no_overlap() {
        let a = range("[0:0_10:0)");
        let b = range("[20:0_30:0)");
        assert!(a.intersect(&b).is_never());
    }

    #[test]
    fn intersect_with_eternity() {
        let a = range("[5:0_15:0)");
        let i = a.intersect(&TimeRange::eternity());
        assert_eq!(i.to_string(), "[5:0_15:0)");
    }

    #[test]
    fn intersect_with_never() {
        let a = range("[0:0_10:0)");
        assert!(a.intersect(&TimeRange::never()).is_never());
    }

    // -- Covers --

    #[test]
    fn covers_fully_contained() {
        assert!(range("[0:0_20:0)").covers(&range("[5:0_15:0)")));
    }

    #[test]
    fn covers_exact_match() {
        assert!(range("[0:0_10:0)").covers(&range("[0:0_10:0)")));
    }

    #[test]
    fn covers_partial_overlap_not_covered() {
        assert!(!range("[5:0_25:0)").covers(&range("[0:0_10:0)")));
    }

    #[test]
    fn covers_no_overlap_not_covered() {
        assert!(!range("[0:0_10:0)").covers(&range("[20:0_30:0)")));
    }

    #[test]
    fn covers_eternity_covers_everything() {
        assert!(range("_").covers(&range("[0:0_10:0)")));
    }

    #[test]
    fn covers_never_is_covered_by_any_range() {
        // Never is trivially covered (empty set is subset of everything)
        assert!(range("[0:0_10:0)").covers(&TimeRange::never()));
    }

    #[test]
    fn covers_boundary_exclusive_end() {
        // [0:0_20:0) does NOT cover [10:0_20:0] because 20:0 inclusive isn't in [0:0_20:0)
        assert!(!range("[0:0_20:0)").covers(&range("[10:0_20:0]")));
    }

    // -- Serde --

    #[test]
    fn serde_serializes_as_string() {
        let tr = range("[0:0_10:0)");
        let json = serde_json::to_string(&tr).unwrap();
        assert_eq!(json, "\"[0:0_10:0)\"");
    }

    #[test]
    fn serde_does_not_produce_object() {
        let tr = range("[0:0_10:0)");
        let json = serde_json::to_string(&tr).unwrap();
        assert!(
            !json.contains("Range"),
            "should not serialize as object: {}",
            json
        );
    }

    #[test]
    fn serde_round_trip() {
        let cases = ["_", "()", "[0:0_10:0)", "[10:0]"];
        for case in cases {
            let tr = range(case);
            let json = serde_json::to_string(&tr).unwrap();
            let tr2: TimeRange = serde_json::from_str(&json).unwrap();
            assert_eq!(tr, tr2, "serde round-trip failed for '{}'", case);
        }
    }
}

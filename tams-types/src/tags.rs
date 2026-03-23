use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A single tag value: either a string or an array of strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TagValue {
    Single(String),
    Multiple(Vec<String>),
}

/// Tags map: keys are freeform strings, values are string or string[].
pub type Tags = HashMap<String, TagValue>;

impl TagValue {
    /// Check if this tag value matches any of the given filter values (OR semantics).
    pub fn matches_any(&self, filter_values: &[&str]) -> bool {
        match self {
            TagValue::Single(v) => filter_values.contains(&v.as_str()),
            TagValue::Multiple(vs) => vs.iter().any(|v| filter_values.contains(&v.as_str())),
        }
    }
}

/// Parsed tag filters extracted from query string.
#[derive(Debug, Default)]
pub struct TagFilters {
    /// tag.{name} = comma-separated values (OR match)
    pub value_filters: Vec<(String, Vec<String>)>,
    /// tag_exists.{name} = true/false
    pub exists_filters: Vec<(String, bool)>,
}

impl TagFilters {
    /// Parse tag filters from a raw query string using the standard "tag." prefix.
    /// Used directly in tests; production code uses extract module functions.
    #[cfg(test)]
    pub fn from_query(query: &str) -> Self {
        Self::from_query_with_prefix(query, "tag")
    }

    /// Parse tag filters from a raw query string with a configurable prefix.
    /// E.g. prefix="tag" matches "tag.{name}" and "tag_exists.{name}".
    /// prefix="flow_tag" matches "flow_tag.{name}" and "flow_tag_exists.{name}".
    pub fn from_query_with_prefix(query: &str, prefix: &str) -> Self {
        let value_prefix = format!("{prefix}.");
        let exists_prefix = format!("{prefix}_exists.");
        let mut value_filters = Vec::new();
        let mut exists_filters = Vec::new();

        for pair in query.split('&') {
            let Some((key, value)) = pair.split_once('=') else {
                continue;
            };
            if let Some(name) = key.strip_prefix(&value_prefix) {
                let values: Vec<String> = value.split(',').map(String::from).collect();
                value_filters.push((name.to_string(), values));
            } else if let Some(name) = key.strip_prefix(&exists_prefix) {
                let exists = value == "true";
                exists_filters.push((name.to_string(), exists));
            }
        }

        Self {
            value_filters,
            exists_filters,
        }
    }

    /// Check if a tags map passes all filters (AND across filters, OR within values).
    pub fn matches(&self, tags: &Tags) -> bool {
        for (name, filter_values) in &self.value_filters {
            let filter_refs: Vec<&str> = filter_values.iter().map(|s| s.as_str()).collect();
            match tags.get(name) {
                Some(tag_value) => {
                    if !tag_value.matches_any(&filter_refs) {
                        return false;
                    }
                }
                None => return false,
            }
        }

        for (name, should_exist) in &self.exists_filters {
            let exists = tags.contains_key(name);
            if exists != *should_exist {
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- TagValue --

    #[test]
    fn single_matches_exact() {
        let v = TagValue::Single("news".into());
        assert!(v.matches_any(&["news"]));
    }

    #[test]
    fn single_no_match() {
        let v = TagValue::Single("news".into());
        assert!(!v.matches_any(&["sport"]));
    }

    #[test]
    fn single_matches_one_of_many() {
        let v = TagValue::Single("news".into());
        assert!(v.matches_any(&["sport", "news"]));
    }

    #[test]
    fn multiple_matches_one_element() {
        let v = TagValue::Multiple(vec!["news".into(), "live".into()]);
        assert!(v.matches_any(&["live"]));
    }

    #[test]
    fn multiple_no_match() {
        let v = TagValue::Multiple(vec!["news".into(), "live".into()]);
        assert!(!v.matches_any(&["sport"]));
    }

    #[test]
    fn multiple_matches_or_across_filter_and_value() {
        let v = TagValue::Multiple(vec!["news".into(), "live".into()]);
        assert!(v.matches_any(&["sport", "news"]));
    }

    // -- TagValue serde --

    #[test]
    fn serde_single_string() {
        let v: TagValue = serde_json::from_str(r#""news""#).unwrap();
        assert_eq!(v, TagValue::Single("news".into()));
        assert_eq!(serde_json::to_string(&v).unwrap(), r#""news""#);
    }

    #[test]
    fn serde_array() {
        let v: TagValue = serde_json::from_str(r#"["news","sport"]"#).unwrap();
        assert_eq!(v, TagValue::Multiple(vec!["news".into(), "sport".into()]));
    }

    // -- TagFilters --

    #[test]
    fn parse_tag_value_filter() {
        let f = TagFilters::from_query("tag.genre=news,sport");
        assert_eq!(f.value_filters.len(), 1);
        assert_eq!(f.value_filters[0].0, "genre");
        assert_eq!(f.value_filters[0].1, vec!["news", "sport"]);
    }

    #[test]
    fn parse_tag_exists_true() {
        let f = TagFilters::from_query("tag_exists.genre=true");
        assert_eq!(f.exists_filters.len(), 1);
        assert_eq!(f.exists_filters[0], ("genre".into(), true));
    }

    #[test]
    fn parse_tag_exists_false() {
        let f = TagFilters::from_query("tag_exists.genre=false");
        assert_eq!(f.exists_filters[0], ("genre".into(), false));
    }

    #[test]
    fn parse_ignores_non_tag_params() {
        let f = TagFilters::from_query("label=foo&tag.genre=news&limit=10");
        assert_eq!(f.value_filters.len(), 1);
        assert!(f.exists_filters.is_empty());
    }

    #[test]
    fn matches_value_filter_pass() {
        let mut tags = Tags::new();
        tags.insert("genre".into(), TagValue::Single("news".into()));
        let f = TagFilters {
            value_filters: vec![("genre".into(), vec!["news".into(), "sport".into()])],
            exists_filters: vec![],
        };
        assert!(f.matches(&tags));
    }

    #[test]
    fn matches_value_filter_fail() {
        let mut tags = Tags::new();
        tags.insert("genre".into(), TagValue::Single("drama".into()));
        let f = TagFilters {
            value_filters: vec![("genre".into(), vec!["news".into(), "sport".into()])],
            exists_filters: vec![],
        };
        assert!(!f.matches(&tags));
    }

    #[test]
    fn matches_exists_true_pass() {
        let mut tags = Tags::new();
        tags.insert("genre".into(), TagValue::Single("news".into()));
        let f = TagFilters {
            value_filters: vec![],
            exists_filters: vec![("genre".into(), true)],
        };
        assert!(f.matches(&tags));
    }

    #[test]
    fn matches_exists_true_fail() {
        let tags = Tags::new();
        let f = TagFilters {
            value_filters: vec![],
            exists_filters: vec![("genre".into(), true)],
        };
        assert!(!f.matches(&tags));
    }

    #[test]
    fn matches_exists_false_pass() {
        let tags = Tags::new();
        let f = TagFilters {
            value_filters: vec![],
            exists_filters: vec![("genre".into(), false)],
        };
        assert!(f.matches(&tags));
    }

    #[test]
    fn matches_exists_false_fail() {
        let mut tags = Tags::new();
        tags.insert("genre".into(), TagValue::Single("news".into()));
        let f = TagFilters {
            value_filters: vec![],
            exists_filters: vec![("genre".into(), false)],
        };
        assert!(!f.matches(&tags));
    }

    #[test]
    fn matches_empty_filters_pass_anything() {
        let tags = Tags::new();
        let f = TagFilters::default();
        assert!(f.matches(&tags));
    }
}

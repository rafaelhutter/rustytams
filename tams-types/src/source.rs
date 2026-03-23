use serde::{Deserialize, Serialize};

use crate::tags::Tags;

/// Source resource matching schemas/source.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Tags>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_collection: Option<Vec<CollectionItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collected_by: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionItem {
    pub id: String,
    pub role: String,
}

/// Query filters for GET /sources.
#[derive(Debug, Default)]
pub struct SourceFilters {
    pub label: Option<String>,
    pub format: Option<String>,
}

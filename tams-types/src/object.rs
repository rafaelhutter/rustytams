use crate::tags::TagFilters;
use crate::timerange::TimeRange;

/// Query parameters for GET /objects/{objectId} (URL-filtering only; pagination handled by handler).
pub struct ObjectQuery {
    pub presigned: Option<bool>,
    pub accept_get_urls: Option<Vec<String>>,
    pub accept_storage_ids: Option<Vec<String>>,
    pub verbose_storage: bool,
    pub flow_tag_filters: TagFilters,
}

/// The store's response for GET /objects/{objectId}.
/// Pagination of `referenced_by_flows` is done in the handler layer.
pub struct ObjectInfo {
    pub id: String,
    pub referenced_by_flows: Vec<String>,
    pub first_referenced_by_flow: Option<String>,
    pub timerange: TimeRange,
    pub get_urls: Vec<serde_json::Value>,
}

/// An uncontrolled instance of a media object (registered by client).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UncontrolledInstance {
    pub url: String,
    pub label: String,
}

/// Instance registration request (oneOf).
pub enum InstanceRequest {
    Controlled { storage_id: String },
    Uncontrolled { url: String, label: String },
}

/// Selector for DELETE /objects/{objectId}/instances.
pub enum InstanceSelector<'a> {
    ByStorageId(&'a str),
    ByLabel(&'a str),
}

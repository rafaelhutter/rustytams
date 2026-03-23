use crate::error::DeletionRequest;
use crate::tags::Tags;

/// Typed fields extracted from flow JSON for filtering and server management.
/// The full flow document is stored as a serde_json::Value alongside this.
/// Only fields that are READ in production code belong here — everything else
/// lives only in the JSON document.
#[derive(Debug, Clone)]
pub struct FlowCore {
    // Used for filtering
    pub source_id: String,
    pub format: String,
    pub codec: Option<String>,
    pub label: Option<String>,
    pub tags: Option<Tags>,
    pub read_only: bool,
    pub timerange: Option<String>,
    pub frame_width: Option<i64>,
    pub frame_height: Option<i64>,
    // Used for server-managed logic
    pub created: Option<String>,
    pub segments_updated: Option<String>,
    pub created_by: Option<String>,
    pub flow_collection: Option<serde_json::Value>,
    pub collected_by: Option<Vec<String>>,
    // Used by storage POST validation
    pub container: Option<String>,
}

/// A stored flow: typed core for filtering + full JSON document for pass-through.
#[derive(Debug, Clone)]
pub struct StoredFlow {
    pub core: FlowCore,
    pub document: serde_json::Value,
}

/// Result of a flow deletion.
#[derive(Debug)]
pub enum DeleteResult {
    /// Flow deleted synchronously (no segments or fast cleanup).
    Deleted,
    /// Flow not found.
    NotFound,
    /// Deletion started asynchronously — returns the deletion request.
    Async(Box<DeletionRequest>),
}

/// Query filters for GET /flows.
#[derive(Debug, Default)]
pub struct FlowFilters {
    pub source_id: Option<String>,
    pub format: Option<String>,
    pub codec: Option<String>,
    pub label: Option<String>,
    pub frame_width: Option<i64>,
    pub frame_height: Option<i64>,
    pub timerange: Option<String>,
}

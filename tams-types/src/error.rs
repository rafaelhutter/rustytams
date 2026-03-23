/// Errors returned by store write operations.
#[derive(Debug)]
pub enum StoreError {
    NotFound(String),
    ReadOnly,
    BadRequest(String),
    Internal(String),
}

/// Status lifecycle for deletion requests.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeletionStatus {
    Created,
    Started,
    Done,
    Error,
}

/// A deletion request for async flow deletion, matching schemas/deletion-request.json.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeletionRequest {
    pub id: String,
    pub flow_id: String,
    pub timerange_to_delete: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timerange_remaining: Option<String>,
    pub delete_flow: bool,
    pub status: DeletionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
}

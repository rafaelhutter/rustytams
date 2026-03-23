use crate::timerange::TimeRange;

/// A stored segment with parsed timerange for overlap detection and filtering.
pub struct StoredSegment {
    pub timerange: TimeRange,
    pub object_id: String,
    pub document: serde_json::Value,
}

/// A segment that failed validation during bulk POST.
pub struct FailedSegment {
    pub object_id: String,
    pub timerange: Option<String>,
    pub error: String,
}

/// Query parameters for GET /flows/{flowId}/segments.
#[derive(Default)]
pub struct SegmentQuery {
    pub timerange: Option<TimeRange>,
    pub object_id: Option<String>,
    pub reverse_order: bool,
    /// Filter get_urls by presigned status (None = no filter).
    pub presigned: Option<bool>,
    /// Filter get_urls by label (None = no filter, Some(empty) = remove all).
    pub accept_get_urls: Option<Vec<String>>,
    /// Filter get_urls by storage_id (None or Some(empty) = no filter).
    pub accept_storage_ids: Option<Vec<String>>,
    /// Include object_timerange in response if it differs from segment timerange.
    pub include_object_timerange: bool,
    /// Include full storage metadata in get_urls (default false = url, presigned, label only).
    pub verbose_storage: bool,
}

/// Result of a segment POST operation.
pub enum SegmentPostResult {
    /// All segments created successfully (201).
    AllCreated,
    /// Some segments failed (200 with failed_segments).
    PartialFailure(Vec<FailedSegment>),
}

/// Result of a segment DELETE operation.
pub enum SegmentDeleteResult {
    /// Segments deleted synchronously (204).
    Deleted,
    /// Deletion started asynchronously — returns the deletion request (202).
    Async(Box<crate::error::DeletionRequest>),
}

/// Storage allocation request.
pub struct StorageRequest {
    pub limit: Option<u64>,
    pub object_ids: Option<Vec<String>>,
    /// Optional storage backend identifier.
    pub storage_id: Option<String>,
}

/// An allocated media object with upload URL.
pub struct AllocatedObject {
    pub object_id: String,
    pub put_url: String,
    pub content_type: Option<String>,
}

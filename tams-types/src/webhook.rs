use serde::{Deserialize, Serialize};

/// Webhook status lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookStatus {
    Created,
    Started,
    Disabled,
    Error,
}

/// Valid event types for webhook subscriptions.
pub const VALID_EVENT_TYPES: &[&str] = &[
    "flows/created",
    "flows/updated",
    "flows/deleted",
    "flows/segments_added",
    "flows/segments_deleted",
    "sources/created",
    "sources/updated",
    "sources/deleted",
];

/// A stored webhook registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredWebhook {
    pub id: String,
    pub url: String,
    pub events: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_name: Option<String>,
    /// Write-only: stored for dispatch but stripped from GET responses by the handler.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_collected_by_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_collected_by_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept_get_urls: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept_storage_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presigned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbose_storage: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<crate::tags::Tags>,
    pub status: WebhookStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
}

/// An event dispatched through the webhook system.
#[derive(Debug, Clone)]
pub enum StoreEvent {
    FlowCreated {
        flow: serde_json::Value,
        source_id: String,
        /// Which flow collections this flow belongs to (for flow_collected_by_ids filter).
        flow_collected_by: Vec<String>,
        /// Which source collections the source belongs to (for source_collected_by_ids filter).
        source_collected_by: Vec<String>,
    },
    FlowUpdated {
        flow: serde_json::Value,
        source_id: String,
        flow_collected_by: Vec<String>,
        source_collected_by: Vec<String>,
    },
    FlowDeleted {
        flow_id: String,
        source_id: String,
        flow_collected_by: Vec<String>,
        source_collected_by: Vec<String>,
    },
    SegmentsAdded {
        flow_id: String,
        source_id: String,
        segments: Vec<serde_json::Value>,
        flow_collected_by: Vec<String>,
        source_collected_by: Vec<String>,
    },
    SegmentsDeleted {
        flow_id: String,
        source_id: String,
        timerange: String,
        flow_collected_by: Vec<String>,
        source_collected_by: Vec<String>,
    },
    SourceCreated {
        source: serde_json::Value,
        source_collected_by: Vec<String>,
    },
    SourceUpdated {
        source: serde_json::Value,
        source_collected_by: Vec<String>,
    },
    SourceDeleted {
        source_id: String,
        source_collected_by: Vec<String>,
    },
}

impl StoreEvent {
    /// The event type string for this event.
    pub fn event_type(&self) -> &str {
        match self {
            StoreEvent::FlowCreated { .. } => "flows/created",
            StoreEvent::FlowUpdated { .. } => "flows/updated",
            StoreEvent::FlowDeleted { .. } => "flows/deleted",
            StoreEvent::SegmentsAdded { .. } => "flows/segments_added",
            StoreEvent::SegmentsDeleted { .. } => "flows/segments_deleted",
            StoreEvent::SourceCreated { .. } => "sources/created",
            StoreEvent::SourceUpdated { .. } => "sources/updated",
            StoreEvent::SourceDeleted { .. } => "sources/deleted",
        }
    }

    /// Build the JSON payload for this event.
    pub fn to_payload(&self) -> serde_json::Value {
        let event_data = match self {
            StoreEvent::FlowCreated { flow, .. } => serde_json::json!({ "flow": flow }),
            StoreEvent::FlowUpdated { flow, .. } => serde_json::json!({ "flow": flow }),
            StoreEvent::FlowDeleted { flow_id, .. } => serde_json::json!({ "flow_id": flow_id }),
            StoreEvent::SegmentsAdded {
                flow_id, segments, ..
            } => serde_json::json!({ "flow_id": flow_id, "segments": segments }),
            StoreEvent::SegmentsDeleted {
                flow_id, timerange, ..
            } => serde_json::json!({ "flow_id": flow_id, "timerange": timerange }),
            StoreEvent::SourceCreated { source, .. } => serde_json::json!({ "source": source }),
            StoreEvent::SourceUpdated { source, .. } => serde_json::json!({ "source": source }),
            StoreEvent::SourceDeleted { source_id, .. } => serde_json::json!({ "source_id": source_id }),
        };
        serde_json::json!({
            "event_timestamp": chrono::Utc::now().to_rfc3339(),
            "event_type": self.event_type(),
            "event": event_data,
        })
    }

    /// Get the flow_id for this event, if applicable.
    pub fn flow_id(&self) -> Option<&str> {
        match self {
            StoreEvent::FlowCreated { flow, .. } | StoreEvent::FlowUpdated { flow, .. } => {
                flow.get("id").and_then(|v| v.as_str())
            }
            StoreEvent::FlowDeleted { flow_id, .. }
            | StoreEvent::SegmentsAdded { flow_id, .. }
            | StoreEvent::SegmentsDeleted { flow_id, .. } => Some(flow_id),
            _ => None,
        }
    }

    /// Get the source_id for this event.
    pub fn source_id(&self) -> Option<&str> {
        match self {
            StoreEvent::FlowCreated { source_id, .. }
            | StoreEvent::FlowUpdated { source_id, .. }
            | StoreEvent::FlowDeleted { source_id, .. }
            | StoreEvent::SegmentsAdded { source_id, .. }
            | StoreEvent::SegmentsDeleted { source_id, .. } => Some(source_id),
            StoreEvent::SourceCreated { source, .. } | StoreEvent::SourceUpdated { source, .. } => {
                source.get("id").and_then(|v| v.as_str())
            }
            StoreEvent::SourceDeleted { source_id, .. } => Some(source_id),
        }
    }

    /// Get the flow_collected_by list for this event.
    pub fn flow_collected_by(&self) -> &[String] {
        match self {
            StoreEvent::FlowCreated {
                flow_collected_by, ..
            }
            | StoreEvent::FlowUpdated {
                flow_collected_by, ..
            }
            | StoreEvent::FlowDeleted {
                flow_collected_by, ..
            }
            | StoreEvent::SegmentsAdded {
                flow_collected_by, ..
            }
            | StoreEvent::SegmentsDeleted {
                flow_collected_by, ..
            } => flow_collected_by,
            _ => &[],
        }
    }

    /// Get the source_collected_by list for this event.
    pub fn source_collected_by(&self) -> &[String] {
        match self {
            StoreEvent::FlowCreated {
                source_collected_by,
                ..
            }
            | StoreEvent::FlowUpdated {
                source_collected_by,
                ..
            }
            | StoreEvent::FlowDeleted {
                source_collected_by,
                ..
            }
            | StoreEvent::SegmentsAdded {
                source_collected_by,
                ..
            }
            | StoreEvent::SegmentsDeleted {
                source_collected_by,
                ..
            }
            | StoreEvent::SourceCreated {
                source_collected_by,
                ..
            }
            | StoreEvent::SourceUpdated {
                source_collected_by,
                ..
            }
            | StoreEvent::SourceDeleted {
                source_collected_by,
                ..
            } => source_collected_by,
        }
    }
}

/// Check if a webhook should receive a given event based on its filters.
pub fn webhook_matches_event(webhook: &StoredWebhook, event: &StoreEvent) -> bool {
    // Must be subscribed to this event type
    if !webhook.events.iter().any(|e| e == event.event_type()) {
        return false;
    }

    // Must be in active state
    if !matches!(
        webhook.status,
        WebhookStatus::Created | WebhookStatus::Started
    ) {
        return false;
    }

    // flow_ids filter (applies to flow and segment events only)
    if let Some(ref filter_ids) = webhook.flow_ids {
        if let Some(flow_id) = event.flow_id() {
            if !filter_ids.iter().any(|id| id == flow_id) {
                return false;
            }
        }
    }

    // source_ids filter (applies to all events)
    if let Some(ref filter_ids) = webhook.source_ids {
        if let Some(source_id) = event.source_id() {
            if !filter_ids.iter().any(|id| id == source_id) {
                return false;
            }
        }
    }

    // flow_collected_by_ids filter (applies to flow and segment events)
    if let Some(ref filter_ids) = webhook.flow_collected_by_ids {
        let collected_by = event.flow_collected_by();
        // Only apply to flow/segment events (not source events)
        if event.flow_id().is_some()
            && !filter_ids
                .iter()
                .any(|id| collected_by.iter().any(|cb| cb == id))
        {
            return false;
        }
    }

    // source_collected_by_ids filter (applies to all events)
    if let Some(ref filter_ids) = webhook.source_collected_by_ids {
        let collected_by = event.source_collected_by();
        if !filter_ids
            .iter()
            .any(|id| collected_by.iter().any(|cb| cb == id))
        {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_webhook(events: Vec<&str>) -> StoredWebhook {
        StoredWebhook {
            id: "wh-1".into(),
            url: "http://example.com/hook".into(),
            events: events.into_iter().map(String::from).collect(),
            api_key_name: None,
            api_key_value: None,
            flow_ids: None,
            source_ids: None,
            flow_collected_by_ids: None,
            source_collected_by_ids: None,
            accept_get_urls: None,
            accept_storage_ids: None,
            presigned: None,
            verbose_storage: None,
            tags: None,
            status: WebhookStatus::Created,
            error: None,
        }
    }

    fn flow_created_event() -> StoreEvent {
        StoreEvent::FlowCreated {
            flow: serde_json::json!({"id": "f1"}),
            source_id: "s1".into(),
            flow_collected_by: vec![],
            source_collected_by: vec![],
        }
    }

    fn flow_created_with_collections(flow_cb: Vec<&str>, source_cb: Vec<&str>) -> StoreEvent {
        StoreEvent::FlowCreated {
            flow: serde_json::json!({"id": "f1"}),
            source_id: "s1".into(),
            flow_collected_by: flow_cb.into_iter().map(String::from).collect(),
            source_collected_by: source_cb.into_iter().map(String::from).collect(),
        }
    }

    fn source_created_event() -> StoreEvent {
        StoreEvent::SourceCreated {
            source: serde_json::json!({"id": "s1"}),
            source_collected_by: vec![],
        }
    }

    fn source_created_with_collections(source_cb: Vec<&str>) -> StoreEvent {
        StoreEvent::SourceCreated {
            source: serde_json::json!({"id": "s1"}),
            source_collected_by: source_cb.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn matches_subscribed_event() {
        let wh = test_webhook(vec!["flows/created"]);
        assert!(webhook_matches_event(&wh, &flow_created_event()));
    }

    #[test]
    fn rejects_unsubscribed_event() {
        let wh = test_webhook(vec!["flows/deleted"]);
        assert!(!webhook_matches_event(&wh, &flow_created_event()));
    }

    #[test]
    fn rejects_disabled_webhook() {
        let mut wh = test_webhook(vec!["flows/created"]);
        wh.status = WebhookStatus::Disabled;
        assert!(!webhook_matches_event(&wh, &flow_created_event()));
    }

    #[test]
    fn rejects_error_webhook() {
        let mut wh = test_webhook(vec!["flows/created"]);
        wh.status = WebhookStatus::Error;
        assert!(!webhook_matches_event(&wh, &flow_created_event()));
    }

    #[test]
    fn flow_ids_filter_matches() {
        let mut wh = test_webhook(vec!["flows/created"]);
        wh.flow_ids = Some(vec!["f1".into()]);
        assert!(webhook_matches_event(&wh, &flow_created_event()));
    }

    #[test]
    fn flow_ids_filter_rejects() {
        let mut wh = test_webhook(vec!["flows/created"]);
        wh.flow_ids = Some(vec!["f2".into()]);
        assert!(!webhook_matches_event(&wh, &flow_created_event()));
    }

    #[test]
    fn flow_ids_filter_no_effect_on_source_events() {
        let mut wh = test_webhook(vec!["sources/created"]);
        wh.flow_ids = Some(vec!["f1".into()]);
        assert!(webhook_matches_event(&wh, &source_created_event()));
    }

    #[test]
    fn source_ids_filter_matches() {
        let mut wh = test_webhook(vec!["flows/created"]);
        wh.source_ids = Some(vec!["s1".into()]);
        assert!(webhook_matches_event(&wh, &flow_created_event()));
    }

    #[test]
    fn source_ids_filter_rejects() {
        let mut wh = test_webhook(vec!["flows/created"]);
        wh.source_ids = Some(vec!["s2".into()]);
        assert!(!webhook_matches_event(&wh, &flow_created_event()));
    }

    #[test]
    fn source_ids_filter_on_source_event() {
        let mut wh = test_webhook(vec!["sources/created"]);
        wh.source_ids = Some(vec!["s1".into()]);
        assert!(webhook_matches_event(&wh, &source_created_event()));
    }

    // -- flow_collected_by_ids filter --

    #[test]
    fn flow_collected_by_ids_matches() {
        let mut wh = test_webhook(vec!["flows/created"]);
        wh.flow_collected_by_ids = Some(vec!["collection-1".into()]);
        let event = flow_created_with_collections(vec!["collection-1"], vec![]);
        assert!(webhook_matches_event(&wh, &event));
    }

    #[test]
    fn flow_collected_by_ids_rejects() {
        let mut wh = test_webhook(vec!["flows/created"]);
        wh.flow_collected_by_ids = Some(vec!["collection-1".into()]);
        let event = flow_created_with_collections(vec!["other-collection"], vec![]);
        assert!(!webhook_matches_event(&wh, &event));
    }

    #[test]
    fn flow_collected_by_ids_rejects_empty() {
        let mut wh = test_webhook(vec!["flows/created"]);
        wh.flow_collected_by_ids = Some(vec!["collection-1".into()]);
        // Flow not in any collection
        assert!(!webhook_matches_event(&wh, &flow_created_event()));
    }

    #[test]
    fn flow_collected_by_ids_no_effect_on_source_events() {
        let mut wh = test_webhook(vec!["sources/created"]);
        wh.flow_collected_by_ids = Some(vec!["collection-1".into()]);
        // Source events don't have flow_collected_by — should pass
        assert!(webhook_matches_event(&wh, &source_created_event()));
    }

    // -- source_collected_by_ids filter --

    #[test]
    fn source_collected_by_ids_matches_flow_event() {
        let mut wh = test_webhook(vec!["flows/created"]);
        wh.source_collected_by_ids = Some(vec!["src-col-1".into()]);
        let event = flow_created_with_collections(vec![], vec!["src-col-1"]);
        assert!(webhook_matches_event(&wh, &event));
    }

    #[test]
    fn source_collected_by_ids_rejects_flow_event() {
        let mut wh = test_webhook(vec!["flows/created"]);
        wh.source_collected_by_ids = Some(vec!["src-col-1".into()]);
        // Flow event with source not in any source collection
        assert!(!webhook_matches_event(&wh, &flow_created_event()));
    }

    #[test]
    fn source_collected_by_ids_matches_source_event() {
        let mut wh = test_webhook(vec!["sources/created"]);
        wh.source_collected_by_ids = Some(vec!["src-col-1".into()]);
        let event = source_created_with_collections(vec!["src-col-1"]);
        assert!(webhook_matches_event(&wh, &event));
    }

    #[test]
    fn source_collected_by_ids_rejects_source_event() {
        let mut wh = test_webhook(vec!["sources/created"]);
        wh.source_collected_by_ids = Some(vec!["src-col-1".into()]);
        assert!(!webhook_matches_event(&wh, &source_created_event()));
    }

    // -- Payload tests --

    #[test]
    fn event_payload_has_required_fields() {
        let event = StoreEvent::FlowCreated {
            flow: serde_json::json!({"id": "f1", "source_id": "s1"}),
            source_id: "s1".into(),
            flow_collected_by: vec![],
            source_collected_by: vec![],
        };
        let payload = event.to_payload();
        assert_eq!(payload["event_type"], "flows/created");
        assert!(payload["event_timestamp"].as_str().is_some());
        assert_eq!(payload["event"]["flow"]["id"], "f1");
    }

    #[test]
    fn event_payload_deleted_has_flow_id_only() {
        let event = StoreEvent::FlowDeleted {
            flow_id: "f1".into(),
            source_id: "s1".into(),
            flow_collected_by: vec![],
            source_collected_by: vec![],
        };
        let payload = event.to_payload();
        assert_eq!(payload["event_type"], "flows/deleted");
        assert_eq!(payload["event"]["flow_id"], "f1");
        assert!(payload["event"].get("flow").is_none());
    }

    #[test]
    fn segments_added_payload_has_segments() {
        let event = StoreEvent::SegmentsAdded {
            flow_id: "f1".into(),
            source_id: "s1".into(),
            segments: vec![serde_json::json!({"timerange": "[0:0_5:0)"})],
            flow_collected_by: vec![],
            source_collected_by: vec![],
        };
        let payload = event.to_payload();
        assert_eq!(payload["event_type"], "flows/segments_added");
        assert_eq!(payload["event"]["flow_id"], "f1");
        assert_eq!(payload["event"]["segments"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn segments_deleted_payload_has_timerange() {
        let event = StoreEvent::SegmentsDeleted {
            flow_id: "f1".into(),
            source_id: "s1".into(),
            timerange: "[0:0_5:0)".into(),
            flow_collected_by: vec![],
            source_collected_by: vec![],
        };
        let payload = event.to_payload();
        assert_eq!(payload["event_type"], "flows/segments_deleted");
        assert_eq!(payload["event"]["timerange"], "[0:0_5:0)");
    }

    #[test]
    fn source_updated_payload() {
        let event = StoreEvent::SourceUpdated {
            source: serde_json::json!({"id": "s1", "format": "urn:x-nmos:format:video"}),
            source_collected_by: vec![],
        };
        let payload = event.to_payload();
        assert_eq!(payload["event_type"], "sources/updated");
        assert_eq!(payload["event"]["source"]["id"], "s1");
    }
}

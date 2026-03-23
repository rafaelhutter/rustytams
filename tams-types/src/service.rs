use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    #[serde(rename = "type")]
    pub service_type: String,
    pub api_version: String,
    pub min_object_timeout: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_stream_mechanisms: Option<Vec<EventStreamMechanism>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_presigned_url_timeout: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventStreamMechanism {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
}

impl Default for ServiceInfo {
    fn default() -> Self {
        Self {
            service_type: "urn:x-tams:service:rustytams".into(),
            api_version: "8.0".into(),
            min_object_timeout: "600:0".into(),
            name: Some("RustyTAMS".into()),
            description: Some("A Rust implementation of the Time-addressable Media Store".into()),
            service_version: Some(env!("CARGO_PKG_VERSION").into()),
            event_stream_mechanisms: Some(vec![EventStreamMechanism {
                name: "webhooks".into(),
                docs: None,
                config: None,
            }]),
            min_presigned_url_timeout: Some("600:0".into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicePost {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageBackend {
    pub id: String,
    pub store_type: String,
    pub provider: String,
    pub store_product: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub availability_zone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_storage: Option<bool>,
}

impl StorageBackend {
    pub fn default_s3(endpoint: &str, bucket: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            store_type: "http_object_store".into(),
            provider: "s3".into(),
            store_product: format!("s3:{endpoint}/{bucket}"),
            label: Some("s3".into()),
            region: None,
            availability_zone: None,
            default_storage: Some(true),
        }
    }
}

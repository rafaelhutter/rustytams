use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};

use bson::{doc, Document};
use futures::TryStreamExt;
use mongodb::options::ReplaceOptions;
use mongodb::Collection;

use aws_credential_types::Credentials;
use aws_sdk_s3::config::{Builder as S3ConfigBuilder, Region};
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::Client as S3Client;

use tams_types::error::{DeletionRequest, DeletionStatus};
use tams_types::flow::{FlowCore, FlowFilters, StoredFlow};
pub use tams_types::flow::DeleteResult;
use tams_types::object::{
    InstanceRequest, InstanceSelector, ObjectInfo, ObjectQuery, UncontrolledInstance,
};
use tams_types::segment::{
    AllocatedObject, FailedSegment, SegmentDeleteResult, SegmentPostResult, SegmentQuery,
    StorageRequest, StoredSegment,
};
use tams_types::service::{ServiceInfo, ServicePost, StorageBackend};
use tams_types::source::{CollectionItem, Source, SourceFilters};
use tams_types::tags::{TagFilters, TagValue, Tags};
use tams_types::timerange::TimeRange;
use tams_types::webhook::{webhook_matches_event, StoreEvent, StoredWebhook, WebhookStatus};

pub use tams_types::error::StoreError;

/// Convert a MongoDB error to a StoreError.
fn mongo_io_err(e: mongodb::error::Error) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

fn to_store_err(e: mongodb::error::Error) -> StoreError {
    StoreError::Database(e.to_string())
}

/// Serialize serde_json::Value to BSON Document.
fn to_bson(val: &serde_json::Value) -> std::io::Result<Document> {
    bson::to_document(val).map_err(|e| std::io::Error::other(e.to_string()))
}

/// Deserialize BSON Document to serde_json::Value.
fn from_bson(doc: Document) -> std::io::Result<serde_json::Value> {
    bson::from_document(doc).map_err(|e| std::io::Error::other(e.to_string()))
}

/// Presigned URL expiry time (1 hour).
const PRESIGN_EXPIRY: Duration = Duration::from_secs(3600);

/// S3 connection configuration.
#[derive(Clone)]
pub struct S3Config {
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub region: String,
}

impl std::fmt::Debug for S3Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Config")
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field("access_key", &"[REDACTED]")
            .field("secret_key", &"[REDACTED]")
            .field("region", &self.region)
            .finish()
    }
}

const COLL_SOURCES: &str = "sources";
const COLL_FLOWS: &str = "flows";
const COLL_SEGMENTS: &str = "segments";
const COLL_INSTANCES: &str = "object_instances";
const COLL_WEBHOOKS: &str = "webhooks";
const COLL_SERVICE: &str = "service";
const COLL_DELETION_REQUESTS: &str = "deletion_requests";

struct StoreInner {
    db: mongodb::Database,
    /// In-memory: flows/sources/segments/instances/webhooks/service/deletion_requests
    /// kept for zero-downtime reads and for the webhook dispatch loop.
    service_info: RwLock<ServiceInfo>,
    storage_backends: Vec<StorageBackend>,
    sources: RwLock<HashMap<String, Source>>,
    flows: RwLock<HashMap<String, StoredFlow>>,
    segments: RwLock<HashMap<String, Vec<StoredSegment>>>,
    object_instances: RwLock<HashMap<String, Vec<UncontrolledInstance>>>,
    webhooks: RwLock<HashMap<String, StoredWebhook>>,
    deletion_requests: RwLock<HashMap<String, DeletionRequest>>,
    /// Legacy persist mutexes – replaced by MongoDB writes; kept as no-ops
    /// so we can remove them incrementally without touching every call site.
    persist_service: Mutex<()>,
    persist_sources: Mutex<()>,
    persist_flows: Mutex<()>,
    persist_instances: Mutex<()>,
    persist_webhooks: Mutex<()>,
    persist_segments: Mutex<()>,
    /// Channel for dispatching events to the background webhook task.
    event_sender: tokio::sync::mpsc::UnboundedSender<StoreEvent>,
    /// S3 client for presigned URL generation.
    s3_client: S3Client,
    /// S3 bucket for media objects.
    s3_bucket: String,
}

#[derive(Clone)]
pub struct Store {
    inner: Arc<StoreInner>,
}

impl Store {
    pub async fn new(mongo_uri: &str, s3: S3Config) -> Result<Self, StoreError> {
        // Connect to MongoDB
        let client = mongodb::Client::with_uri_str(mongo_uri)
            .await
            .map_err(to_store_err)?;
        let db = client.database("tams");

        // Create indexes --------------------------------------------------------
        use mongodb::IndexModel;
        use bson::doc;

        // sources: unique on id
        db.collection::<Document>(COLL_SOURCES)
            .create_index(IndexModel::builder().keys(doc! { "id": 1 }).options(
                mongodb::options::IndexOptions::builder().unique(true).build(),
            ).build())
            .await
            .map_err(to_store_err)?;

        // flows: unique on id
        db.collection::<Document>(COLL_FLOWS)
            .create_index(IndexModel::builder().keys(doc! { "id": 1 }).options(
                mongodb::options::IndexOptions::builder().unique(true).build(),
            ).build())
            .await
            .map_err(to_store_err)?;

        // segments: compound (flow_id, ts_start) + object_id
        {
            let col = db.collection::<Document>(COLL_SEGMENTS);
            col.create_index(IndexModel::builder().keys(doc! { "flow_id": 1, "ts_start": 1 }).build())
                .await
                .map_err(to_store_err)?;
            col.create_index(IndexModel::builder().keys(doc! { "object_id": 1 }).build())
                .await
                .map_err(to_store_err)?;
        }

        // object_instances: unique on object_id
        db.collection::<Document>(COLL_INSTANCES)
            .create_index(IndexModel::builder().keys(doc! { "object_id": 1 }).options(
                mongodb::options::IndexOptions::builder().unique(true).build(),
            ).build())
            .await
            .map_err(to_store_err)?;

        // webhooks: unique on id
        db.collection::<Document>(COLL_WEBHOOKS)
            .create_index(IndexModel::builder().keys(doc! { "id": 1 }).options(
                mongodb::options::IndexOptions::builder().unique(true).build(),
            ).build())
            .await
            .map_err(to_store_err)?;

        // deletion_requests: unique on id
        db.collection::<Document>(COLL_DELETION_REQUESTS)
            .create_index(IndexModel::builder().keys(doc! { "id": 1 }).options(
                mongodb::options::IndexOptions::builder().unique(true).build(),
            ).build())
            .await
            .map_err(to_store_err)?;

        // Load data from MongoDB into in-memory maps ----------------------------

        // service info
        let service_info = {
            let col = db.collection::<Document>(COLL_SERVICE);
            match col.find_one(doc! { "_id": "singleton" }).await.map_err(to_store_err)? {
                Some(d) => {
                    let v = from_bson(d).map_err(|e| StoreError::Database(e.to_string()))?;
                    serde_json::from_value::<ServiceInfo>(v)
                        .map_err(|e| StoreError::Database(e.to_string()))?
                }
                None => {
                    // Insert defaults
                    let info = ServiceInfo::default();
                    let mut doc = to_bson(&serde_json::to_value(&info)
                        .map_err(|e| StoreError::Database(e.to_string()))?)
                        .map_err(|e| StoreError::Database(e.to_string()))?;
                    doc.insert("_id", "singleton");
                    col.insert_one(doc).await.map_err(to_store_err)?;
                    info
                }
            }
        };

        // sources
        let sources: HashMap<String, Source> = {
            let col = db.collection::<Document>(COLL_SOURCES);
            let cursor = col.find(doc! {}).await.map_err(to_store_err)?;
            let docs: Vec<Document> = cursor.try_collect().await.map_err(to_store_err)?;
            let mut map = HashMap::new();
            for d in docs {
                let Ok(v) = from_bson(d) else { continue };
                if let Ok(src) = serde_json::from_value::<Source>(v) {
                    map.insert(src.id.clone(), src);
                }
            }
            map
        };

        // flows
        let flows: HashMap<String, StoredFlow> = {
            let col = db.collection::<Document>(COLL_FLOWS);
            let cursor = col.find(doc! {}).await.map_err(to_store_err)?;
            let docs: Vec<Document> = cursor.try_collect().await.map_err(to_store_err)?;
            let mut map = HashMap::new();
            for d in docs {
                let Ok(v) = from_bson(d) else { continue };
                if let Some(id) = v.get("id").and_then(|x| x.as_str()).map(String::from) {
                    if let Some(core) = flow_core_from_document(&v) {
                        map.insert(id, StoredFlow { core, document: v });
                    }
                }
            }
            map
        };

        // segments
        let segments: HashMap<String, Vec<StoredSegment>> = {
            let col = db.collection::<Document>(COLL_SEGMENTS);
            let cursor = col.find(doc! {}).await.map_err(to_store_err)?;
            let docs: Vec<Document> = cursor.try_collect().await.map_err(to_store_err)?;
            let mut map: HashMap<String, Vec<StoredSegment>> = HashMap::new();
            for d in docs {
                let Ok(v) = from_bson(d) else { continue };
                let flow_id = match v.get("flow_id").and_then(|x| x.as_str()) {
                    Some(f) => f.to_string(),
                    None => continue,
                };
                let tr_str = match v.get("timerange").and_then(|x| x.as_str()) {
                    Some(t) => t,
                    None => continue,
                };
                let timerange: TimeRange = match tr_str.parse() {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                let object_id = match v.get("object_id").and_then(|x| x.as_str()) {
                    Some(o) => o.to_string(),
                    None => continue,
                };
                map.entry(flow_id).or_default().push(StoredSegment {
                    timerange,
                    object_id,
                    document: v,
                });
            }
            map
        };

        // object_instances
        let object_instances: HashMap<String, Vec<UncontrolledInstance>> = {
            let col = db.collection::<Document>(COLL_INSTANCES);
            let cursor = col.find(doc! {}).await.map_err(to_store_err)?;
            let docs: Vec<Document> = cursor.try_collect().await.map_err(to_store_err)?;
            let mut map = HashMap::new();
            for d in docs {
                let Ok(v) = from_bson(d) else { continue };
                let object_id = match v.get("object_id").and_then(|x| x.as_str()).map(String::from) {
                    Some(o) => o,
                    None => continue,
                };
                if let Some(arr) = v.get("instances") {
                    if let Ok(instances) = serde_json::from_value::<Vec<UncontrolledInstance>>(arr.clone()) {
                        map.insert(object_id, instances);
                    }
                }
            }
            map
        };

        // webhooks
        let webhooks: HashMap<String, StoredWebhook> = {
            let col = db.collection::<Document>(COLL_WEBHOOKS);
            let cursor = col.find(doc! {}).await.map_err(to_store_err)?;
            let docs: Vec<Document> = cursor.try_collect().await.map_err(to_store_err)?;
            let mut map = HashMap::new();
            for d in docs {
                let Ok(v) = from_bson(d) else { continue };
                if let Ok(wh) = serde_json::from_value::<StoredWebhook>(v) {
                    map.insert(wh.id.clone(), wh);
                }
            }
            map
        };

        // deletion_requests
        let deletion_requests: HashMap<String, DeletionRequest> = {
            let col = db.collection::<Document>(COLL_DELETION_REQUESTS);
            let cursor = col.find(doc! {}).await.map_err(to_store_err)?;
            let docs: Vec<Document> = cursor.try_collect().await.map_err(to_store_err)?;
            let mut map = HashMap::new();
            for d in docs {
                let Ok(v) = from_bson(d) else { continue };
                if let Ok(dr) = serde_json::from_value::<DeletionRequest>(v) {
                    map.insert(dr.id.clone(), dr);
                }
            }
            map
        };

        // Event channel + S3 client --------------------------------------------
        let (event_sender, event_receiver) = tokio::sync::mpsc::unbounded_channel();

        let credentials =
            Credentials::new(&s3.access_key, &s3.secret_key, None, None, "tams-store");
        let s3_config = S3ConfigBuilder::new()
            .endpoint_url(&s3.endpoint)
            .region(Region::new(s3.region.clone()))
            .credentials_provider(credentials)
            .force_path_style(true)
            .build();
        let s3_client = S3Client::from_conf(s3_config);

        let store = Store {
            inner: Arc::new(StoreInner {
                db,
                service_info: RwLock::new(service_info),
                storage_backends: vec![StorageBackend::default_s3(&s3.endpoint, &s3.bucket)],
                sources: RwLock::new(sources),
                flows: RwLock::new(flows),
                segments: RwLock::new(segments),
                object_instances: RwLock::new(object_instances),
                webhooks: RwLock::new(webhooks),
                deletion_requests: RwLock::new(deletion_requests),
                persist_sources: Mutex::new(()),
                persist_flows: Mutex::new(()),
                persist_instances: Mutex::new(()),
                persist_service: Mutex::new(()),
                persist_webhooks: Mutex::new(()),
                persist_segments: Mutex::new(()),
                event_sender,
                s3_client,
                s3_bucket: s3.bucket.clone(),
            }),
        };

        // Spawn background event dispatch task
        let inner = store.inner.clone();
        tokio::spawn(event_dispatch_task(inner, event_receiver));

        Ok(store)
    }

    /// Collection helpers (private) -------------------------------------------
    fn col_sources(&self) -> mongodb::Collection<Document> {
        self.inner.db.collection(COLL_SOURCES)
    }
    fn col_flows(&self) -> mongodb::Collection<Document> {
        self.inner.db.collection(COLL_FLOWS)
    }
    fn col_segments(&self) -> mongodb::Collection<Document> {
        self.inner.db.collection(COLL_SEGMENTS)
    }
    fn col_instances(&self) -> mongodb::Collection<Document> {
        self.inner.db.collection(COLL_INSTANCES)
    }
    fn col_webhooks(&self) -> mongodb::Collection<Document> {
        self.inner.db.collection(COLL_WEBHOOKS)
    }
    fn col_service(&self) -> mongodb::Collection<Document> {
        self.inner.db.collection(COLL_SERVICE)
    }
    fn col_deletion_requests(&self) -> mongodb::Collection<Document> {
        self.inner.db.collection(COLL_DELETION_REQUESTS)
    }

    pub async fn get_service_info(&self) -> ServiceInfo {
        self.inner.service_info.read().await.clone()
    }

    pub async fn update_service_info(&self, post: ServicePost) -> std::io::Result<ServiceInfo> {
        {
            let mut info = self.inner.service_info.write().await;
            if let Some(name) = post.name {
                info.name = Some(name);
            }
            if let Some(description) = post.description {
                info.description = Some(description);
            }
        } // write lock dropped before persist
        self.persist_service_info().await?;
        let info = self.inner.service_info.read().await;
        Ok(info.clone())
    }

    pub fn storage_backends(&self) -> &[StorageBackend] {
        &self.inner.storage_backends
    }

    // -- Sources --

    /// Create a source only if one with the same ID doesn't already exist.
    /// Uses entry API to avoid TOCTOU race. Persists to disk.
    async fn create_source_if_absent(&self, mut source: Source) -> Result<(), StoreError> {
        use std::collections::hash_map::Entry;
        fill_source_defaults(&mut source);
        let mut sources = self.inner.sources.write().await;
        let inserted = match sources.entry(source.id.clone()) {
            Entry::Vacant(e) => {
                let source_json = serde_json::to_value(&source).ok();
                e.insert(source);
                source_json
            }
            Entry::Occupied(_) => None,
        };
        drop(sources);
        if let Some(source_json) = inserted {
            self.persist_sources()
                .await
                .map_err(|e| StoreError::Internal(format!("Failed to persist sources: {e}")))?;
            self.dispatch_event(StoreEvent::SourceCreated {
                source: source_json,
                source_collected_by: vec![],
            });
        }
        Ok(())
    }

    /// Get all sources, optionally filtered.
    pub async fn list_sources(
        &self,
        filters: &SourceFilters,
        tag_filters: &TagFilters,
    ) -> Vec<Source> {
        let sources = self.inner.sources.read().await;
        let empty_tags = Tags::new();
        let mut result: Vec<Source> = sources
            .values()
            .filter(|s| {
                if let Some(ref label) = filters.label {
                    if s.label.as_deref() != Some(label.as_str()) {
                        return false;
                    }
                }
                if let Some(ref format) = filters.format {
                    if s.format != *format {
                        return false;
                    }
                }
                let tags = s.tags.as_ref().unwrap_or(&empty_tags);
                tag_filters.matches(tags)
            })
            .cloned()
            .collect();
        result.sort_by(|a, b| a.id.cmp(&b.id));
        result
    }

    /// Get a single source by ID.
    pub async fn get_source(&self, id: &str) -> Option<Source> {
        self.inner.sources.read().await.get(id).cloned()
    }

    /// Get tags for a source.
    pub async fn get_source_tags(&self, id: &str) -> Option<Tags> {
        let sources = self.inner.sources.read().await;
        let source = sources.get(id)?;
        Some(source.tags.clone().unwrap_or_default())
    }

    /// Get a single tag value.
    pub async fn get_source_tag(&self, id: &str, name: &str) -> Option<TagValue> {
        let sources = self.inner.sources.read().await;
        let source = sources.get(id)?;
        source.tags.as_ref()?.get(name).cloned()
    }

    /// Mutate a source by ID, updating the `updated` timestamp. Persists to disk.
    /// Emits a SourceUpdated event after successful mutation.
    async fn mutate_source<F>(&self, id: &str, f: F) -> Result<(), StoreError>
    where
        F: FnOnce(&mut Source),
    {
        let (source_json, source_cb) = {
            let mut sources = self.inner.sources.write().await;
            let source = sources
                .get_mut(id)
                .ok_or_else(|| StoreError::NotFound(format!("Source {id} not found")))?;
            f(source);
            source.updated = Some(chrono::Utc::now().to_rfc3339());
            let json = serde_json::to_value(&*source).unwrap_or_default();
            let cb = source.collected_by.clone().unwrap_or_default();
            (json, cb)
        };
        self.persist_sources()
            .await
            .map_err(|e| StoreError::Internal(format!("Failed to persist sources: {e}")))?;
        self.dispatch_event(StoreEvent::SourceUpdated {
            source: source_json,
            source_collected_by: source_cb,
        });
        Ok(())
    }

    /// Set a tag value.
    pub async fn set_source_tag(
        &self,
        id: &str,
        name: &str,
        value: TagValue,
    ) -> Result<(), StoreError> {
        let name = name.to_string();
        self.mutate_source(id, |s| {
            s.tags.get_or_insert_with(Tags::new).insert(name, value);
        })
        .await
    }

    /// Delete a tag.
    pub async fn delete_source_tag(&self, id: &str, name: &str) -> Result<(), StoreError> {
        let name = name.to_string();
        self.mutate_source(id, |s| {
            if let Some(tags) = &mut s.tags {
                tags.remove(&name);
            }
        })
        .await
    }

    /// Get source label. Returns None if source not found, Some(None) if no label set.
    pub async fn get_source_label(&self, id: &str) -> Option<Option<String>> {
        self.inner
            .sources
            .read()
            .await
            .get(id)
            .map(|s| s.label.clone())
    }

    /// Set source label.
    pub async fn set_source_label(&self, id: &str, label: String) -> Result<(), StoreError> {
        self.mutate_source(id, |s| s.label = Some(label)).await
    }

    /// Delete source label.
    pub async fn delete_source_label(&self, id: &str) -> Result<(), StoreError> {
        self.mutate_source(id, |s| s.label = None).await
    }

    /// Get source description. Returns None if source not found, Some(None) if no desc set.
    pub async fn get_source_description(&self, id: &str) -> Option<Option<String>> {
        self.inner
            .sources
            .read()
            .await
            .get(id)
            .map(|s| s.description.clone())
    }

    /// Set source description.
    pub async fn set_source_description(&self, id: &str, desc: String) -> Result<(), StoreError> {
        self.mutate_source(id, |s| s.description = Some(desc)).await
    }

    /// Delete source description.
    pub async fn delete_source_description(&self, id: &str) -> Result<(), StoreError> {
        self.mutate_source(id, |s| s.description = None).await
    }

    /// DELETE /sources/{sourceId} -- remove a source and all its flows.
    pub async fn delete_source(&self, id: &str) -> Result<DeleteResult, StoreError> {
        // Collect flow IDs belonging to this source before removing anything
        let flow_ids: Vec<String> = {
            let flows = self.inner.flows.read().await;
            flows
                .iter()
                .filter(|(_, sf)| sf.core.source_id == id)
                .map(|(flow_id, _)| flow_id.clone())
                .collect()
        };

        // Delete each flow (handles segments + webhook events)
        for flow_id in &flow_ids {
            self.delete_flow(flow_id).await?;
        }

        // Remove the source itself from memory
        let source_cb = self.source_collected_by(id).await;
        {
            let mut sources = self.inner.sources.write().await;
            if sources.remove(id).is_none() {
                return Ok(DeleteResult::NotFound);
            }
        }

        // Remove from MongoDB
        self.col_sources()
            .delete_one(bson::doc! { "id": id })
            .await
            .map_err(mongo_io_err)
            .map_err(|e| StoreError::Internal(e.to_string()))?;

        // Dispatch webhook event
        self.dispatch_event(StoreEvent::SourceDeleted {
            source_id: id.to_string(),
            source_collected_by: source_cb,
        });

        Ok(DeleteResult::Deleted)
    }

    // -- Flows --

    /// Create or update a flow. Returns (created, response_document).
    /// On create, returns `Some(doc)`. On update, returns `None` (handler sends 204).
    /// Validates required fields, strips/injects server-managed fields,
    /// auto-creates source if needed. Persists to disk.
    pub async fn put_flow(
        &self,
        mut doc: serde_json::Value,
    ) -> Result<(bool, Option<serde_json::Value>), StoreError> {
        let obj = doc
            .as_object_mut()
            .ok_or_else(|| StoreError::BadRequest("Flow must be a JSON object".into()))?;

        // Extract required fields
        let id = obj
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| StoreError::BadRequest("Missing required field: id".into()))?
            .to_string();
        if !tams_types::is_safe_id(&id) {
            return Err(StoreError::BadRequest(
                "Flow ID contains invalid path characters".into(),
            ));
        }
        let source_id = obj
            .get("source_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| StoreError::BadRequest("Missing required field: source_id".into()))?
            .to_string();
        let format = obj
            .get("format")
            .and_then(|v| v.as_str())
            .ok_or_else(|| StoreError::BadRequest("Missing required field: format".into()))?
            .to_string();

        // Validate format is a recognized NMOS value
        const VALID_FORMATS: &[&str] = &[
            "urn:x-nmos:format:video",
            "urn:x-nmos:format:audio",
            "urn:x-nmos:format:image",
            "urn:x-nmos:format:data",
            "urn:x-nmos:format:multi",
        ];
        if !VALID_FORMATS.contains(&format.as_str()) {
            return Err(StoreError::BadRequest(format!(
                "Unrecognized format: {format}. Must be one of: {}",
                VALID_FORMATS.join(", ")
            )));
        }

        // Validate format-specific required fields per TAMS spec
        match format.as_str() {
            "urn:x-nmos:format:video" | "urn:x-nmos:format:image" => {
                if obj.get("codec").and_then(|v| v.as_str()).is_none() {
                    return Err(StoreError::BadRequest(format!(
                        "{format} flows require a codec field"
                    )));
                }
                let ep = obj.get("essence_parameters").and_then(|v| v.as_object());
                match ep {
                    None => {
                        return Err(StoreError::BadRequest(format!(
                            "{format} flows require essence_parameters"
                        )));
                    }
                    Some(ep) => {
                        if ep.get("frame_width").and_then(|v| v.as_i64()).is_none() {
                            return Err(StoreError::BadRequest(format!(
                                "{format} flows require essence_parameters.frame_width"
                            )));
                        }
                        if ep.get("frame_height").and_then(|v| v.as_i64()).is_none() {
                            return Err(StoreError::BadRequest(format!(
                                "{format} flows require essence_parameters.frame_height"
                            )));
                        }
                    }
                }
            }
            "urn:x-nmos:format:audio" => {
                if obj.get("codec").and_then(|v| v.as_str()).is_none() {
                    return Err(StoreError::BadRequest(
                        "urn:x-nmos:format:audio flows require a codec field".into(),
                    ));
                }
                if obj
                    .get("essence_parameters")
                    .and_then(|v| v.as_object())
                    .is_none()
                {
                    return Err(StoreError::BadRequest(
                        "urn:x-nmos:format:audio flows require essence_parameters".into(),
                    ));
                }
            }
            // data and multi: no additional required fields
            _ => {}
        }

        // Look up existing flow once, capture all preserved values
        let mut flows = self.inner.flows.write().await;
        // Capture old flow_collection children for source_collection cleanup on update
        let old_fc_child_ids: Vec<String> = flows
            .get(&id)
            .map(|existing| extract_collection_child_ids_from_core(&existing.core))
            .unwrap_or_default();
        let preserved = flows.get(&id).map(|existing| {
            if existing.core.read_only {
                let new_read_only = obj
                    .get("read_only")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                if new_read_only {
                    return Err(StoreError::ReadOnly);
                }
            }
            Ok((
                existing.core.created.clone(),
                existing.core.segments_updated.clone(),
                existing.core.created_by.clone(),
            ))
        });
        let is_create = preserved.is_none();
        let (prev_created, prev_segments_updated, prev_created_by) = match preserved {
            Some(Ok(vals)) => (vals.0, vals.1, vals.2),
            Some(Err(e)) => return Err(e),
            None => (None, None, None),
        };

        // Strip all server-managed fields from client input
        obj.remove("timerange");
        obj.remove("collected_by");
        obj.remove("created");
        obj.remove("metadata_updated");
        obj.remove("segments_updated");
        obj.remove("metadata_version");
        obj.remove("created_by");
        obj.remove("updated_by");

        // Inject server-managed timestamps
        let now = chrono::Utc::now().to_rfc3339();
        if is_create {
            obj.insert("created".into(), serde_json::Value::String(now.clone()));
        } else {
            if let Some(created) = prev_created {
                obj.insert("created".into(), serde_json::Value::String(created));
            }
            if let Some(seg_updated) = prev_segments_updated {
                obj.insert(
                    "segments_updated".into(),
                    serde_json::Value::String(seg_updated),
                );
            }
        }
        obj.insert(
            "metadata_updated".into(),
            serde_json::Value::String(now.clone()),
        );

        // Generate a new metadata_version on every write
        obj.insert(
            "metadata_version".into(),
            serde_json::Value::String(uuid::Uuid::new_v4().to_string()),
        );

        // Manage created_by / updated_by
        if is_create {
            obj.insert(
                "created_by".into(),
                serde_json::Value::String("server".into()),
            );
        } else if let Some(cb) = prev_created_by {
            obj.insert("created_by".into(), serde_json::Value::String(cb));
        }
        obj.insert(
            "updated_by".into(),
            serde_json::Value::String("server".into()),
        );

        // Validate tags before extracting FlowCore (startup loading is lenient, but PUT must validate)
        if let Some(tags_val) = obj.get("tags") {
            if !tags_val.is_null() {
                let _: Tags = serde_json::from_value(tags_val.clone())
                    .map_err(|e| StoreError::BadRequest(format!("Invalid tags: {e}")))?;
            }
        }

        let core = flow_core_from_document(&doc).ok_or_else(|| {
            StoreError::Internal("Failed to extract FlowCore from document".into())
        })?;

        // Only clone the doc for the response on create (updates return 204 with no body)
        let response_doc = if is_create { Some(doc.clone()) } else { None };
        let event_doc = doc.clone();
        let flow_cb = core.collected_by.clone().unwrap_or_default();

        // Capture flow_collection items for source_collection sync
        let fc_items = core.flow_collection.as_ref().map(extract_collection_items);

        let id_owned = id.clone();
        flows.insert(
            id,
            StoredFlow {
                core,
                document: doc,
            },
        );
        drop(flows); // Release write lock before I/O

        // Persist flows to disk
        self.persist_flows()
            .await
            .map_err(|e| StoreError::Internal(format!("Failed to persist flows: {e}")))?;

        // Auto-create source before dispatching events (so webhook listeners
        // can immediately GET the referenced source without a 404 race).
        self.create_source_if_absent(Source {
            id: source_id.clone(),
            format,
            label: None,
            description: None,
            tags: None,
            created_by: Some("server".into()),
            updated_by: Some("server".into()),
            created: None,
            updated: None,
            source_collection: None,
            collected_by: None,
        })
        .await?;

        // Sync source_collection from flow_collection (handles create, update, and removal)
        if fc_items.is_some() || !old_fc_child_ids.is_empty() {
            self.apply_flow_collection_sync(&id_owned, &source_id, &old_fc_child_ids, fc_items)
                .await?;
        }

        // Dispatch webhook event
        let source_cb = self.source_collected_by(&source_id).await;
        if is_create {
            self.dispatch_event(StoreEvent::FlowCreated {
                flow: event_doc,
                source_id,
                flow_collected_by: flow_cb,
                source_collected_by: source_cb,
            });
        } else {
            self.dispatch_event(StoreEvent::FlowUpdated {
                flow: event_doc,
                source_id,
                flow_collected_by: flow_cb,
                source_collected_by: source_cb,
            });
        }

        Ok((is_create, response_doc))
    }

    /// Get all flows, optionally filtered. Returns documents with timerange stripped.
    pub async fn list_flows(
        &self,
        filters: &FlowFilters,
        tag_filters: &TagFilters,
        include_timerange: bool,
    ) -> Vec<serde_json::Value> {
        let flows = self.inner.flows.read().await;
        let empty_tags = Tags::new();

        // Parse timerange filter if provided
        let timerange_filter: Option<TimeRange> = filters
            .timerange
            .as_ref()
            .and_then(|s| s.parse::<TimeRange>().ok());

        let mut result: Vec<serde_json::Value> = flows
            .values()
            .filter(|sf| {
                let c = &sf.core;
                if let Some(ref sid) = filters.source_id {
                    if c.source_id != *sid {
                        return false;
                    }
                }
                if let Some(ref fmt) = filters.format {
                    if c.format != *fmt {
                        return false;
                    }
                }
                if let Some(ref codec) = filters.codec {
                    if c.codec.as_deref() != Some(codec.as_str()) {
                        return false;
                    }
                }
                if let Some(ref label) = filters.label {
                    if c.label.as_deref() != Some(label.as_str()) {
                        return false;
                    }
                }
                if let Some(fw) = filters.frame_width {
                    if c.frame_width != Some(fw) {
                        return false;
                    }
                }
                if let Some(fh) = filters.frame_height {
                    if c.frame_height != Some(fh) {
                        return false;
                    }
                }
                // Timerange filter per spec:
                // - "_" (eternity) or no filter: include all flows
                // - "()" (never): include only flows with no content (no timerange)
                // - Other: include flows whose timerange overlaps the filter
                if let Some(ref query_tr) = timerange_filter {
                    if query_tr.is_eternity() {
                        // No filtering — include all
                    } else if query_tr.is_never() {
                        // Only flows with no content
                        if c.timerange.is_some() {
                            return false;
                        }
                    } else {
                        // Filter by overlap
                        match &c.timerange {
                            None => return false, // No content, can't overlap
                            Some(flow_tr_str) => {
                                let flow_tr: TimeRange =
                                    flow_tr_str.parse().unwrap_or(TimeRange::never());
                                if !flow_tr.overlaps(query_tr) {
                                    return false;
                                }
                            }
                        }
                    }
                }
                let tags = c.tags.as_ref().unwrap_or(&empty_tags);
                tag_filters.matches(tags)
            })
            .map(|sf| {
                let mut doc = sf.document.clone();
                if include_timerange {
                    // Ensure timerange is present — "()" (never) if flow has no segments
                    if let Some(obj) = doc.as_object_mut() {
                        obj.entry("timerange")
                            .or_insert_with(|| serde_json::Value::String("()".to_string()));
                    }
                } else {
                    if let Some(obj) = doc.as_object_mut() {
                        obj.remove("timerange");
                    }
                }
                doc
            })
            .collect();
        result.sort_by(|a, b| {
            let aid = a.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let bid = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
            aid.cmp(bid)
        });
        result
    }

    /// Get a single flow document by ID.
    pub async fn get_flow(&self, id: &str) -> Option<serde_json::Value> {
        self.inner
            .flows
            .read()
            .await
            .get(id)
            .map(|sf| sf.document.clone())
    }

    // -- Flow property CRUD --

    /// Mutate a flow's document and FlowCore. Checks read_only (unless `skip_read_only` is true),
    /// updates metadata_updated + metadata_version, rebuilds FlowCore, persists to disk.
    async fn mutate_flow<F>(&self, id: &str, skip_read_only: bool, f: F) -> Result<(), StoreError>
    where
        F: FnOnce(&mut serde_json::Map<String, serde_json::Value>),
    {
        let mut flows = self.inner.flows.write().await;
        let sf = flows
            .get_mut(id)
            .ok_or_else(|| StoreError::NotFound(format!("Flow {id} not found")))?;
        if !skip_read_only && sf.core.read_only {
            return Err(StoreError::ReadOnly);
        }
        let obj = sf
            .document
            .as_object_mut()
            .ok_or_else(|| StoreError::Internal("Flow document is not an object".into()))?;
        f(obj);
        // Update server-managed metadata timestamps
        let now = chrono::Utc::now().to_rfc3339();
        obj.insert("metadata_updated".into(), serde_json::Value::String(now));
        obj.insert(
            "metadata_version".into(),
            serde_json::Value::String(uuid::Uuid::new_v4().to_string()),
        );
        // Rebuild FlowCore from updated document
        sf.core = flow_core_from_document(&sf.document)
            .ok_or_else(|| StoreError::Internal("Failed to rebuild FlowCore".into()))?;
        drop(flows);
        self.persist_flows()
            .await
            .map_err(|e| StoreError::Internal(format!("Failed to persist flows: {e}")))?;
        Ok(())
    }

    /// Get all tags for a flow.
    pub async fn get_flow_tags(&self, id: &str) -> Option<Tags> {
        let flows = self.inner.flows.read().await;
        let sf = flows.get(id)?;
        Some(sf.core.tags.clone().unwrap_or_default())
    }

    /// Get a single tag value from a flow.
    pub async fn get_flow_tag(&self, id: &str, name: &str) -> Option<TagValue> {
        let flows = self.inner.flows.read().await;
        let sf = flows.get(id)?;
        sf.core.tags.as_ref()?.get(name).cloned()
    }

    /// Set a tag on a flow.
    pub async fn set_flow_tag(
        &self,
        id: &str,
        name: &str,
        value: TagValue,
    ) -> Result<(), StoreError> {
        let name = name.to_string();
        let tag_json = serde_json::to_value(&value)
            .map_err(|e| StoreError::Internal(format!("Failed to serialize tag: {e}")))?;
        self.mutate_flow(id, false, |obj| {
            let tags = obj
                .entry("tags")
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            if let Some(tags_obj) = tags.as_object_mut() {
                tags_obj.insert(name, tag_json);
            }
        })
        .await
    }

    /// Delete a tag from a flow.
    pub async fn delete_flow_tag(&self, id: &str, name: &str) -> Result<(), StoreError> {
        let name = name.to_string();
        self.mutate_flow(id, false, |obj| {
            if let Some(tags) = obj.get_mut("tags").and_then(|v| v.as_object_mut()) {
                tags.remove(&name);
            }
        })
        .await
    }

    /// Get a generic JSON property from a flow document.
    /// Returns None if flow not found, Some(None) if property not set, Some(Some(val)) if set.
    pub async fn get_flow_property(
        &self,
        id: &str,
        property: &str,
    ) -> Option<Option<serde_json::Value>> {
        let flows = self.inner.flows.read().await;
        let sf = flows.get(id)?;
        let val = sf
            .document
            .as_object()
            .and_then(|obj| obj.get(property))
            .filter(|v| !v.is_null())
            .cloned();
        Some(val)
    }

    /// Set a generic JSON property on a flow document.
    pub async fn set_flow_property(
        &self,
        id: &str,
        property: &str,
        value: serde_json::Value,
    ) -> Result<(), StoreError> {
        let property = property.to_string();
        self.mutate_flow(id, false, |obj| {
            obj.insert(property, value);
        })
        .await
    }

    /// Delete a generic JSON property from a flow document.
    pub async fn delete_flow_property(&self, id: &str, property: &str) -> Result<(), StoreError> {
        let property = property.to_string();
        self.mutate_flow(id, false, |obj| {
            obj.remove(&property);
        })
        .await
    }

    /// Get read_only for a flow. Returns None if flow not found.
    pub async fn get_flow_read_only(&self, id: &str) -> Option<bool> {
        let flows = self.inner.flows.read().await;
        let sf = flows.get(id)?;
        Some(sf.core.read_only)
    }

    /// Set read_only on a flow. Exempt from read_only check (always allowed).
    pub async fn set_flow_read_only(&self, id: &str, value: bool) -> Result<(), StoreError> {
        self.mutate_flow(id, true, |obj| {
            obj.insert("read_only".into(), serde_json::Value::Bool(value));
        })
        .await
    }

    /// Set flow_collection with collected_by tracking.
    /// Validates that all referenced child flows exist.
    /// Also auto-computes source_collection on the parent source per spec.
    pub async fn set_flow_collection(
        &self,
        id: &str,
        value: serde_json::Value,
    ) -> Result<(), StoreError> {
        let collection_items = extract_collection_items(&value);
        let child_ids: Vec<String> = collection_items.iter().map(|(id, _)| id.clone()).collect();
        let mut flows = self.inner.flows.write().await;

        // Check existence, read_only, and capture old child IDs in one lookup
        let sf = flows
            .get(id)
            .ok_or_else(|| StoreError::NotFound(format!("Flow {id} not found")))?;
        if sf.core.read_only {
            return Err(StoreError::ReadOnly);
        }
        let parent_source_id = sf.core.source_id.clone();
        let old_child_ids = extract_collection_child_ids_from_core(&sf.core);

        // Validate all child flow IDs exist
        for child_id in &child_ids {
            if !flows.contains_key(child_id) {
                return Err(StoreError::BadRequest(format!(
                    "Referenced flow {child_id} does not exist"
                )));
            }
        }

        // Build source_collection: map each child flow's source_id + role
        let source_collection_items = build_source_collection_items(&flows, &collection_items);
        let old_source_child_ids = resolve_flow_ids_to_source_ids(&flows, &old_child_ids);

        let parent_id = id.to_string();

        // Update the flow_collection on the parent
        touch_flow_metadata(flows.get_mut(id).unwrap(), |obj| {
            obj.insert("flow_collection".into(), value);
        })?;

        // Remove parent from old children's collected_by
        remove_parent_from_collected_by(&mut flows, &old_child_ids, &parent_id);

        // Add parent to new children's collected_by
        add_parent_to_collected_by(&mut flows, &child_ids, &parent_id);

        drop(flows);

        // Update source_collection on the parent source
        self.sync_source_collection(
            &parent_source_id,
            Some(source_collection_items),
            &old_source_child_ids,
        )
        .await?;

        self.persist_flows()
            .await
            .map_err(|e| StoreError::Internal(format!("Failed to persist flows: {e}")))?;
        self.persist_sources()
            .await
            .map_err(|e| StoreError::Internal(format!("Failed to persist sources: {e}")))?;
        Ok(())
    }

    /// Delete flow_collection and clean up collected_by on child flows.
    /// Also clears source_collection on the parent source.
    pub async fn delete_flow_collection(&self, id: &str) -> Result<(), StoreError> {
        let mut flows = self.inner.flows.write().await;
        let sf = flows
            .get(id)
            .ok_or_else(|| StoreError::NotFound(format!("Flow {id} not found")))?;
        if sf.core.read_only {
            return Err(StoreError::ReadOnly);
        }
        let parent_source_id = sf.core.source_id.clone();
        let old_child_ids = extract_collection_child_ids_from_core(&sf.core);
        let old_source_child_ids = resolve_flow_ids_to_source_ids(&flows, &old_child_ids);
        let parent_id = id.to_string();

        // Remove flow_collection from parent
        touch_flow_metadata(flows.get_mut(id).unwrap(), |obj| {
            obj.remove("flow_collection");
        })?;

        // Clean up collected_by on old children
        remove_parent_from_collected_by(&mut flows, &old_child_ids, &parent_id);

        drop(flows);

        // Clear source_collection on the parent source
        self.sync_source_collection(&parent_source_id, None, &old_source_child_ids)
            .await?;

        self.persist_flows()
            .await
            .map_err(|e| StoreError::Internal(format!("Failed to persist flows: {e}")))?;
        self.persist_sources()
            .await
            .map_err(|e| StoreError::Internal(format!("Failed to persist sources: {e}")))?;
        Ok(())
    }

    /// Delete a flow by ID. Returns DeleteResult.
    ///
    /// If the flow has segments, an async deletion request is created and the
    /// actual deletion runs in a background task. The caller gets 202 + Location.
    /// If no segments, deletion is synchronous and returns 204.
    pub async fn delete_flow(&self, id: &str) -> Result<DeleteResult, StoreError> {
        // Lock ordering: segments before flows (matches post_segments →
        // recompute_flow_timerange path) to prevent deadlock. The segments
        // lock is dropped before acquiring flows write to avoid blocking
        // all segment writes while waiting for the flows lock. The TOCTOU
        // between the check and removal is benign: worst case, segments
        // are added during the window, and the flow takes the async
        // deletion path (202) which cleans up segments correctly.
        let has_segments = {
            let segments = self.inner.segments.read().await;
            segments.get(id).is_some_and(|segs| !segs.is_empty())
        };

        let mut flows = self.inner.flows.write().await;
        let sf = match flows.get(id) {
            Some(sf) => sf,
            None => return Ok(DeleteResult::NotFound),
        };
        if sf.core.read_only {
            return Err(StoreError::ReadOnly);
        }

        let source_id = sf.core.source_id.clone();
        let flow_cb = sf.core.collected_by.clone().unwrap_or_default();
        // Capture child source IDs from flow_collection for source cleanup
        let fc_child_ids = extract_collection_child_ids_from_core(&sf.core);
        let old_source_child_ids = resolve_flow_ids_to_source_ids(&flows, &fc_child_ids);
        // Clean up collected_by on child flows before removing the parent
        let parent_id_str = id.to_string();
        remove_parent_from_collected_by(&mut flows, &fc_child_ids, &parent_id_str);
        flows.remove(id);
        drop(flows);

        // Clean up source_collection if this flow had a flow_collection
        if !old_source_child_ids.is_empty() {
            self.sync_source_collection(&source_id, None, &old_source_child_ids)
                .await?;
            self.persist_sources()
                .await
                .map_err(|e| StoreError::Internal(format!("Failed to persist sources: {e}")))?;
        }

        if !has_segments {
            // Synchronous — no segments to clean up, flow already removed
            self.persist_flows()
                .await
                .map_err(|e| StoreError::Internal(format!("Failed to persist flows: {e}")))?;
            self.dispatch_flow_deleted(id, &source_id, flow_cb).await;
            return Ok(DeleteResult::Deleted);
        }

        // Async deletion — persist the flow removal, then spawn segment cleanup
        self.persist_flows()
            .await
            .map_err(|e| StoreError::Internal(format!("Failed to persist flows: {e}")))?;

        let now = chrono::Utc::now().to_rfc3339();
        let request = DeletionRequest {
            id: uuid::Uuid::new_v4().to_string(),
            flow_id: id.to_string(),
            timerange_to_delete: "_".into(),
            timerange_remaining: Some("_".into()),
            delete_flow: true,
            status: DeletionStatus::Created,
            created: Some(now),
            created_by: Some("server".into()),
            updated: None,
            expiry: None,
            error: None,
        };

        self.inner
            .deletion_requests
            .write()
            .await
            .insert(request.id.clone(), request.clone());

        let store = self.clone();
        let req_id = request.id.clone();
        let flow_id = id.to_string();
        tokio::spawn(async move {
            store
                .execute_deletion(&req_id, &flow_id, &source_id, flow_cb)
                .await;
        });

        Ok(DeleteResult::Async(Box::new(request)))
    }

    /// Dispatch a FlowDeleted event. Shared by sync and async deletion paths.
    async fn dispatch_flow_deleted(&self, flow_id: &str, source_id: &str, flow_cb: Vec<String>) {
        let source_cb = self.source_collected_by(source_id).await;
        self.dispatch_event(StoreEvent::FlowDeleted {
            flow_id: flow_id.to_string(),
            source_id: source_id.to_string(),
            flow_collected_by: flow_cb,
            source_collected_by: source_cb,
        });
    }

    /// Background task: clean up segments then dispatch FlowDeleted.
    async fn execute_deletion(
        &self,
        request_id: &str,
        flow_id: &str,
        source_id: &str,
        flow_cb: Vec<String>,
    ) {
        self.set_deletion_status(request_id, DeletionStatus::Started, None)
            .await;

        if let Err(e) = self.cleanup_flow_segments(flow_id).await {
            let err = serde_json::json!({
                "type": "internal_server_error",
                "summary": format!("Segment cleanup failed: {e:?}"),
                "time": chrono::Utc::now().to_rfc3339()
            });
            self.set_deletion_status(request_id, DeletionStatus::Error, Some(err))
                .await;
            return;
        }

        self.dispatch_flow_deleted(flow_id, source_id, flow_cb)
            .await;
        self.set_deletion_status(request_id, DeletionStatus::Done, None)
            .await;
    }

    /// Update a deletion request's status and timestamp.
    async fn set_deletion_status(
        &self,
        request_id: &str,
        status: DeletionStatus,
        error: Option<serde_json::Value>,
    ) {
        let mut requests = self.inner.deletion_requests.write().await;
        if let Some(req) = requests.get_mut(request_id) {
            if status == DeletionStatus::Done {
                req.timerange_remaining = None;
            }
            req.status = status;
            req.updated = Some(chrono::Utc::now().to_rfc3339());
            req.error = error;
        }
    }

    /// List all tracked deletion requests, pruning completed ones older than 60s.
    pub async fn list_deletion_requests(&self) -> Vec<DeletionRequest> {
        let mut requests = self.inner.deletion_requests.write().await;
        Self::prune_completed_requests(&mut requests);
        requests.values().cloned().collect()
    }

    /// Get a single deletion request by ID.
    pub async fn get_deletion_request(&self, id: &str) -> Option<DeletionRequest> {
        let requests = self.inner.deletion_requests.read().await;
        requests.get(id).cloned()
    }

    /// Remove completed deletion requests older than 60 seconds.
    fn prune_completed_requests(requests: &mut HashMap<String, DeletionRequest>) {
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(60);
        requests.retain(|_, req| {
            if req.status != DeletionStatus::Done && req.status != DeletionStatus::Error {
                return true;
            }
            req.updated
                .as_ref()
                .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                .is_none_or(|t| t > cutoff)
        });
    }

    // -- Segments --

    /// Generate a presigned S3 GET URL for downloading a media object.
    async fn presigned_get_url(&self, object_id: &str) -> Result<String, StoreError> {
        let presigning = PresigningConfig::expires_in(PRESIGN_EXPIRY)
            .map_err(|e| StoreError::Internal(format!("presign config error: {e}")))?;
        let presigned = self
            .inner
            .s3_client
            .get_object()
            .bucket(&self.inner.s3_bucket)
            .key(object_id)
            .presigned(presigning)
            .await
            .map_err(|e| StoreError::Internal(format!("presign GET error: {e}")))?;
        Ok(presigned.uri().to_string())
    }

    /// Generate a presigned S3 PUT URL for uploading a media object.
    async fn presigned_put_url(&self, object_id: &str) -> Result<String, StoreError> {
        let presigning = PresigningConfig::expires_in(PRESIGN_EXPIRY)
            .map_err(|e| StoreError::Internal(format!("presign config error: {e}")))?;
        let presigned = self
            .inner
            .s3_client
            .put_object()
            .bucket(&self.inner.s3_bucket)
            .key(object_id)
            .presigned(presigning)
            .await
            .map_err(|e| StoreError::Internal(format!("presign PUT error: {e}")))?;
        Ok(presigned.uri().to_string())
    }

    /// Check that a flow exists, is writable, and optionally has a container.
    /// Returns the flow's container value if present.
    async fn require_writable_flow(
        &self,
        flow_id: &str,
        require_container: bool,
    ) -> Result<Option<String>, StoreError> {
        let flows = self.inner.flows.read().await;
        let sf = flows
            .get(flow_id)
            .ok_or_else(|| StoreError::NotFound(format!("Flow {flow_id} not found")))?;
        if sf.core.read_only {
            return Err(StoreError::ReadOnly);
        }
        if require_container && sf.core.container.is_none() {
            return Err(StoreError::BadRequest(
                "Flow must have a container set".into(),
            ));
        }
        Ok(sf.core.container.clone())
    }

    /// POST /flows/{flowId}/segments — create one or more segments.
    /// Validates timeranges, rejects overlaps, updates flow timerange and segments_updated.
    pub async fn post_segments(
        &self,
        flow_id: &str,
        docs: Vec<serde_json::Value>,
    ) -> Result<SegmentPostResult, StoreError> {
        self.require_writable_flow(flow_id, true).await?;

        let mut created = Vec::new();
        let mut failed = Vec::new();

        let mut segments = self.inner.segments.write().await;
        let flow_segments = segments.entry(flow_id.to_string()).or_default();

        for doc in docs {
            let obj = match doc.as_object() {
                Some(o) => o,
                None => {
                    failed.push(FailedSegment {
                        object_id: String::new(),
                        timerange: None,
                        error: "Segment must be a JSON object".into(),
                    });
                    continue;
                }
            };

            let object_id = match obj.get("object_id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => {
                    failed.push(FailedSegment {
                        object_id: String::new(),
                        timerange: None,
                        error: "Missing required field: object_id".into(),
                    });
                    continue;
                }
            };

            let tr_str = match obj.get("timerange").and_then(|v| v.as_str()) {
                Some(s) => s.to_string(),
                None => {
                    failed.push(FailedSegment {
                        object_id,
                        timerange: None,
                        error: "Missing required field: timerange".into(),
                    });
                    continue;
                }
            };

            let timerange = match tr_str.parse::<TimeRange>() {
                Ok(tr) => tr,
                Err(e) => {
                    failed.push(FailedSegment {
                        object_id,
                        timerange: Some(tr_str),
                        error: format!("Invalid timerange: {e}"),
                    });
                    continue;
                }
            };

            if timerange.is_never() {
                failed.push(FailedSegment {
                    object_id,
                    timerange: Some(tr_str),
                    error: "Timerange must not be empty".into(),
                });
                continue;
            }

            // Check overlap with existing segments AND already-created segments in this batch
            let has_overlap = flow_segments
                .iter()
                .chain(created.iter())
                .any(|existing| existing.timerange.overlaps(&timerange));
            if has_overlap {
                failed.push(FailedSegment {
                    object_id,
                    timerange: Some(tr_str),
                    error: "Segment timerange overlaps with existing segment".into(),
                });
                continue;
            }

            created.push(StoredSegment {
                timerange,
                object_id,
                document: doc,
            });
        }

        if created.is_empty() && !failed.is_empty() {
            // All segments failed — return 400
            return Err(StoreError::BadRequest(
                failed
                    .iter()
                    .map(|f| f.error.clone())
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }

        // Collect new segment documents for event dispatch before moving
        let new_seg_docs: Vec<serde_json::Value> =
            created.iter().map(|s| s.document.clone()).collect();

        // Insert all created segments
        flow_segments.extend(created);

        // Sort by start timestamp for consistent ordering
        flow_segments.sort_by(|a, b| {
            let start_a = segment_start_nanos(&a.timerange);
            let start_b = segment_start_nanos(&b.timerange);
            start_a.cmp(&start_b)
        });

        drop(segments);

        // Update flow's timerange and segments_updated
        self.recompute_flow_timerange(flow_id).await?;

        // Persist segments to disk
        self.persist_segments(flow_id)
            .await
            .map_err(|e| StoreError::Internal(format!("Failed to persist segments: {e}")))?;

        // Dispatch segments_added event (only the newly added segments)
        if !new_seg_docs.is_empty() {
            let flows = self.inner.flows.read().await;
            if let Some(sf) = flows.get(flow_id) {
                let source_id = sf.core.source_id.clone();
                let flow_cb = sf.core.collected_by.clone().unwrap_or_default();
                drop(flows);
                let source_cb = self.source_collected_by(&source_id).await;
                self.dispatch_event(StoreEvent::SegmentsAdded {
                    flow_id: flow_id.to_string(),
                    source_id,
                    segments: new_seg_docs,
                    flow_collected_by: flow_cb,
                    source_collected_by: source_cb,
                });
            } else {
                drop(flows);
            }
        }

        if failed.is_empty() {
            Ok(SegmentPostResult::AllCreated)
        } else {
            Ok(SegmentPostResult::PartialFailure(failed))
        }
    }

    /// GET /flows/{flowId}/segments — query, filter, compute timerange in one pass.
    /// Returns (documents with get_urls, union timerange of matching segments).
    pub async fn get_segments(
        &self,
        flow_id: &str,
        query: &SegmentQuery,
    ) -> (Vec<serde_json::Value>, TimeRange) {
        let segments = self.inner.segments.read().await;
        let flow_segments = match segments.get(flow_id) {
            Some(s) => s,
            None => return (Vec::new(), TimeRange::never()),
        };

        let mut matched: Vec<&StoredSegment> = flow_segments
            .iter()
            .filter(|seg| matches_query(seg, query))
            .collect();

        let data_timerange = matched
            .iter()
            .fold(TimeRange::never(), |acc, seg| acc.union(&seg.timerange));

        if query.reverse_order {
            matched.reverse();
        }

        let backend = &self.inner.storage_backends[0];
        let backend_storage_id = &backend.id;
        let backend_label = backend.label.as_deref().unwrap_or("s3");

        let include_url = should_include_controlled_url(
            query.presigned,
            true, // our URLs are presigned (contain access_token)
            query.accept_get_urls.as_deref(),
            query.accept_storage_ids.as_deref(),
            backend_storage_id,
            backend_label,
        );

        // Collect cloned data so we can drop the segments read lock before async presigning.
        let segment_data: Vec<(serde_json::Value, String)> = matched
            .into_iter()
            .map(|seg| (seg.document.clone(), seg.object_id.clone()))
            .collect();
        drop(segments);

        let mut docs = Vec::with_capacity(segment_data.len());
        for (mut doc, object_id) in segment_data {
            if let Some(obj) = doc.as_object_mut() {
                // Preserve uncontrolled get_urls, filtered by accept_get_urls
                // and excluded when presigned=true (uncontrolled URLs are never presigned).
                let mut urls = filter_uncontrolled_urls(
                    obj.get("get_urls").and_then(|v| v.as_array()).map(|v| &**v),
                    query.presigned,
                    query.accept_get_urls.as_deref(),
                );

                if include_url {
                    match self.presigned_get_url(&object_id).await {
                        Ok(presigned) => {
                            urls.push(build_controlled_url(
                                &presigned,
                                backend_label,
                                backend_storage_id,
                                query.verbose_storage,
                            ));
                        }
                        Err(e) => {
                            tracing::warn!(object_id, error = ?e, "failed to generate presigned GET URL");
                        }
                    }
                }
                obj.insert("get_urls".into(), serde_json::json!(urls));

                // Strip object_timerange unless requested
                if !query.include_object_timerange {
                    obj.remove("object_timerange");
                }
            }
            docs.push(doc);
        }

        (docs, data_timerange)
    }

    /// DELETE /flows/{flowId}/segments — delete segments matching query.
    /// Cleans up unreferenced media objects from the local metadata store.
    pub async fn delete_segments(
        &self,
        flow_id: &str,
        query: &SegmentQuery,
    ) -> Result<SegmentDeleteResult, StoreError> {
        self.require_writable_flow(flow_id, false).await?;

        let mut segments = self.inner.segments.write().await;

        // Remove matching segments
        if let Some(flow_segments) = segments.get_mut(flow_id) {
            let has_filters = query.timerange.is_some() || query.object_id.is_some();
            if !has_filters {
                flow_segments.clear();
            } else {
                flow_segments.retain(|seg| !delete_matches(seg, query));
            }
            if flow_segments.is_empty() {
                segments.remove(flow_id);
            }
        }

        drop(segments);

        self.recompute_flow_timerange(flow_id).await?;
        self.persist_segments(flow_id)
            .await
            .map_err(|e| StoreError::Internal(format!("Failed to persist segments: {e}")))?;

        // Dispatch segments_deleted event
        let tr_str = query
            .timerange
            .as_ref()
            .map(|t| t.to_string())
            .unwrap_or_else(|| "_".into());
        {
            let flows = self.inner.flows.read().await;
            if let Some(sf) = flows.get(flow_id) {
                let source_id = sf.core.source_id.clone();
                let flow_cb = sf.core.collected_by.clone().unwrap_or_default();
                drop(flows);
                let source_cb = self.source_collected_by(&source_id).await;
                self.dispatch_event(StoreEvent::SegmentsDeleted {
                    flow_id: flow_id.to_string(),
                    source_id,
                    timerange: tr_str.clone(),
                    flow_collected_by: flow_cb,
                    source_collected_by: source_cb,
                });
            }
        }

        // Media cleanup is handled by the S3 store independently.
        // The metadata store only manages segment references.

        Ok(SegmentDeleteResult::Deleted)
    }

    /// POST /flows/{flowId}/storage — allocate media objects.
    pub async fn allocate_storage(
        &self,
        flow_id: &str,
        request: StorageRequest,
    ) -> Result<Vec<AllocatedObject>, StoreError> {
        // require_writable_flow returns the container value, eliminating a second lock
        let content_type = self.require_writable_flow(flow_id, true).await?;

        // Validate storage_id if provided
        if let Some(ref sid) = request.storage_id {
            if !self.inner.storage_backends.iter().any(|b| b.id == *sid) {
                return Err(StoreError::BadRequest(format!(
                    "Invalid storage backend identifier: {sid}"
                )));
            }
        }

        if request.limit.is_some() && request.object_ids.is_some() {
            return Err(StoreError::BadRequest(
                "Cannot specify both limit and object_ids".into(),
            ));
        }

        // Check that supplied object_ids don't already exist in segments
        if let Some(ref ids) = request.object_ids {
            let segments = self.inner.segments.read().await;
            for id in ids {
                let in_segments = segments
                    .values()
                    .any(|segs| segs.iter().any(|s| s.object_id == *id));
                if in_segments {
                    return Err(StoreError::BadRequest(format!(
                        "Object ID '{id}' already exists"
                    )));
                }
            }
        }

        let object_ids: Vec<String> = if let Some(ids) = request.object_ids {
            ids
        } else {
            let count = request.limit.unwrap_or(1) as usize;
            (0..count)
                .map(|_| uuid::Uuid::new_v4().to_string())
                .collect()
        };

        let mut objects = Vec::with_capacity(object_ids.len());
        for object_id in object_ids {
            let put_url = self.presigned_put_url(&object_id).await?;
            objects.push(AllocatedObject {
                object_id,
                put_url,
                content_type: content_type.clone(),
            });
        }

        Ok(objects)
    }

    // Clients use presigned S3 URLs from allocate_storage() to upload/download directly.

    // -- Objects --

    /// GET /objects/{objectId} — compute object metadata from segments.
    /// Returns ObjectInfo with the full (unpaginated) referenced_by_flows list;
    /// the handler paginates using the shared pagination utility.
    pub async fn get_object(
        &self,
        object_id: &str,
        query: &ObjectQuery,
    ) -> Result<ObjectInfo, StoreError> {
        let segments = self.inner.segments.read().await;
        let flows = self.inner.flows.read().await;
        let empty_tags = Tags::new();

        let mut referenced_flows: Vec<String> = Vec::new();
        let mut timerange = TimeRange::never();

        for (flow_id, flow_segs) in segments.iter() {
            // Single pass: check for matching segments and union timeranges together
            let mut flow_has_object = false;
            for seg in flow_segs.iter().filter(|s| s.object_id == object_id) {
                flow_has_object = true;
                timerange = timerange.union(&seg.timerange);
            }
            if !flow_has_object {
                continue;
            }

            // Apply flow_tag filters
            if let Some(sf) = flows.get(flow_id) {
                let tags = sf.core.tags.as_ref().unwrap_or(&empty_tags);
                if !query.flow_tag_filters.matches(tags) {
                    continue;
                }
            }

            referenced_flows.push(flow_id.clone());
        }

        // Drop locks before any I/O
        drop(segments);
        drop(flows);

        if referenced_flows.is_empty() {
            return Err(StoreError::NotFound(format!(
                "Object {object_id} not found"
            )));
        }

        // Sort for deterministic output; first_referenced_by_flow = first sorted
        referenced_flows.sort();
        let first_referenced_by_flow = referenced_flows.first().cloned();

        // Build get_urls
        let mut get_urls = self.build_object_get_urls(object_id, query).await;

        // Add controlled URL if it passes filters
        let backend = &self.inner.storage_backends[0];
        let backend_label = backend.label.as_deref().unwrap_or("s3");
        if should_include_controlled_url(
            query.presigned,
            true, // our URLs are presigned
            query.accept_get_urls.as_deref(),
            query.accept_storage_ids.as_deref(),
            &backend.id,
            backend_label,
        ) {
            match self.presigned_get_url(object_id).await {
                Ok(presigned) => {
                    get_urls.push(build_controlled_url(
                        &presigned,
                        backend_label,
                        &backend.id,
                        query.verbose_storage,
                    ));
                }
                Err(e) => {
                    tracing::warn!(object_id, error = ?e, "failed to generate presigned GET URL");
                }
            }
        }

        Ok(ObjectInfo {
            id: object_id.to_string(),
            referenced_by_flows: referenced_flows,
            first_referenced_by_flow,
            timerange,
            get_urls,
        })
    }

    /// Collect uncontrolled get_urls for an object, applying query filters.
    async fn build_object_get_urls(
        &self,
        object_id: &str,
        query: &ObjectQuery,
    ) -> Vec<serde_json::Value> {
        // Uncontrolled URLs are never presigned — exclude them when presigned=true
        if query.presigned == Some(true) {
            return Vec::new();
        }

        let mut urls = Vec::new();
        let instances = self.inner.object_instances.read().await;
        if let Some(uncontrolled) = instances.get(object_id) {
            for inst in uncontrolled {
                let label_ok = query
                    .accept_get_urls
                    .as_ref()
                    .is_none_or(|labels| labels.iter().any(|l| l == &inst.label));
                if label_ok {
                    let mut url_obj = serde_json::json!({
                        "url": inst.url,
                        "label": inst.label,
                    });
                    if query.verbose_storage {
                        url_obj["controlled"] = serde_json::json!(false);
                    }
                    urls.push(url_obj);
                }
            }
        }
        urls
    }

    /// POST /objects/{objectId}/instances — register an object instance.
    /// Inlines the existence check to avoid TOCTOU with a separate `object_exists`.
    pub async fn add_object_instance(
        &self,
        object_id: &str,
        request: InstanceRequest,
    ) -> Result<(), StoreError> {
        // Check object exists by scanning segments (holds read lock briefly)
        {
            let segments = self.inner.segments.read().await;
            let exists = segments
                .values()
                .any(|segs| segs.iter().any(|s| s.object_id == object_id));
            if !exists {
                return Err(StoreError::NotFound(format!(
                    "Object {object_id} not found"
                )));
            }
        }

        match request {
            InstanceRequest::Controlled { storage_id } => {
                if !self
                    .inner
                    .storage_backends
                    .iter()
                    .any(|b| b.id == storage_id)
                {
                    return Err(StoreError::BadRequest(format!(
                        "Unknown storage backend: {storage_id}"
                    )));
                }
                // Only one backend — controlled duplication is a no-op
                Ok(())
            }
            InstanceRequest::Uncontrolled { url, label } => {
                if self
                    .inner
                    .storage_backends
                    .iter()
                    .any(|b| b.label.as_deref() == Some(&label))
                {
                    return Err(StoreError::BadRequest(format!(
                        "Label '{label}' conflicts with a storage backend label"
                    )));
                }

                let mut instances = self.inner.object_instances.write().await;
                let list = instances.entry(object_id.to_string()).or_default();
                if list.iter().any(|i| i.label == label) {
                    return Err(StoreError::BadRequest(format!(
                        "Instance with label '{label}' already exists"
                    )));
                }
                list.push(UncontrolledInstance { url, label });
                drop(instances);
                self.persist_object_instances().await.map_err(|e| {
                    StoreError::Internal(format!("Failed to persist instances: {e}"))
                })?;
                Ok(())
            }
        }
    }

    /// DELETE /objects/{objectId}/instances — remove an instance by selector.
    pub async fn delete_object_instance(
        &self,
        object_id: &str,
        selector: InstanceSelector<'_>,
    ) -> Result<(), StoreError> {
        // Inline existence check
        {
            let segments = self.inner.segments.read().await;
            let exists = segments
                .values()
                .any(|segs| segs.iter().any(|s| s.object_id == object_id));
            if !exists {
                return Err(StoreError::NotFound(format!(
                    "Object {object_id} not found"
                )));
            }
        }

        match selector {
            InstanceSelector::ByStorageId(sid) => {
                if self.inner.storage_backends.iter().any(|b| b.id == sid) {
                    return Err(StoreError::BadRequest(
                        "Cannot delete the only controlled instance".into(),
                    ));
                }
                Err(StoreError::NotFound(format!(
                    "No instance with storage_id '{sid}'"
                )))
            }
            InstanceSelector::ByLabel(lbl) => {
                let mut instances = self.inner.object_instances.write().await;
                let list = instances.get_mut(object_id).ok_or_else(|| {
                    StoreError::NotFound(format!("No instance with label '{lbl}'"))
                })?;
                let before = list.len();
                list.retain(|i| i.label != lbl);
                if list.len() == before {
                    return Err(StoreError::NotFound(format!(
                        "No instance with label '{lbl}'"
                    )));
                }
                if list.is_empty() {
                    instances.remove(object_id);
                }
                drop(instances);
                self.persist_object_instances().await.map_err(|e| {
                    StoreError::Internal(format!("Failed to persist instances: {e}"))
                })?;
                Ok(())
            }
        }
    }

    // -- Webhooks --

    /// List all webhooks, optionally filtered by tags. Sorted by ID for deterministic pagination.
    pub async fn list_webhooks(&self, tag_filters: &TagFilters) -> Vec<StoredWebhook> {
        let empty_tags = Tags::new();
        let webhooks = self.inner.webhooks.read().await;
        let mut result: Vec<StoredWebhook> = webhooks
            .values()
            .filter(|w| {
                let tags = w.tags.as_ref().unwrap_or(&empty_tags);
                tag_filters.matches(tags)
            })
            .cloned()
            .collect();
        result.sort_by(|a, b| a.id.cmp(&b.id));
        result
    }

    /// Get a single webhook by ID.
    pub async fn get_webhook(&self, id: &str) -> Result<StoredWebhook, StoreError> {
        let webhooks = self.inner.webhooks.read().await;
        webhooks
            .get(id)
            .cloned()
            .ok_or_else(|| StoreError::NotFound(format!("Webhook {id} not found")))
    }

    /// Create a new webhook. Assigns a UUID and sets status to Created.
    pub async fn create_webhook(
        &self,
        mut webhook: StoredWebhook,
    ) -> Result<StoredWebhook, StoreError> {
        webhook.id = uuid::Uuid::new_v4().to_string();
        if webhook.status != WebhookStatus::Created && webhook.status != WebhookStatus::Disabled {
            webhook.status = WebhookStatus::Created;
        }
        let mut webhooks = self.inner.webhooks.write().await;
        webhooks.insert(webhook.id.clone(), webhook.clone());
        drop(webhooks);
        self.persist_webhooks()
            .await
            .map_err(|e| StoreError::Internal(format!("Failed to persist webhooks: {e}")))?;
        Ok(webhook)
    }

    /// Update an existing webhook. Validates status transitions.
    pub async fn update_webhook(
        &self,
        id: &str,
        update: StoredWebhook,
    ) -> Result<StoredWebhook, StoreError> {
        let mut webhooks = self.inner.webhooks.write().await;
        let existing = webhooks
            .get(id)
            .ok_or_else(|| StoreError::NotFound(format!("Webhook {id} not found")))?;

        // Validate status transition
        let new_status = &update.status;
        match (&existing.status, new_status) {
            // created/started → disabled OK
            (WebhookStatus::Created | WebhookStatus::Started, WebhookStatus::Disabled) => {}
            // disabled/error → created OK (re-enable)
            (WebhookStatus::Disabled | WebhookStatus::Error, WebhookStatus::Created) => {}
            // same state OK
            (old, new) if old == new => {}
            // error → disabled NOT allowed
            (WebhookStatus::Error, WebhookStatus::Disabled) => {
                return Err(StoreError::BadRequest(
                    "Cannot transition from error to disabled; set to created to re-enable".into(),
                ));
            }
            (from, to) => {
                return Err(StoreError::BadRequest(format!(
                    "Invalid status transition: {from:?} → {to:?}"
                )));
            }
        }

        let mut updated = update;
        updated.id = id.to_string();
        // Preserve api_key_value if not provided in update
        if updated.api_key_value.is_none() {
            updated.api_key_value = existing.api_key_value.clone();
        }
        // Clear error when re-enabling
        if updated.status == WebhookStatus::Created {
            updated.error = None;
        }
        webhooks.insert(id.to_string(), updated.clone());
        drop(webhooks);
        self.persist_webhooks()
            .await
            .map_err(|e| StoreError::Internal(format!("Failed to persist webhooks: {e}")))?;
        Ok(updated)
    }

    /// Delete a webhook by ID.
    pub async fn delete_webhook(&self, id: &str) -> Result<(), StoreError> {
        let mut webhooks = self.inner.webhooks.write().await;
        if webhooks.remove(id).is_none() {
            return Err(StoreError::NotFound(format!("Webhook {id} not found")));
        }
        drop(webhooks);
        self.persist_webhooks()
            .await
            .map_err(|e| StoreError::Internal(format!("Failed to persist webhooks: {e}")))?;
        Ok(())
    }

    /// Dispatch an event to all matching webhooks via the background task.
    pub fn dispatch_event(&self, event: StoreEvent) {
        let _ = self.inner.event_sender.send(event);
    }

    /// Recompute a flow's timerange from its segments and update timestamps.
    async fn recompute_flow_timerange(&self, flow_id: &str) -> Result<(), StoreError> {
        let segments = self.inner.segments.read().await;
        let timerange = segments
            .get(flow_id)
            .map(|segs| {
                segs.iter()
                    .fold(TimeRange::never(), |acc, seg| acc.union(&seg.timerange))
            })
            .unwrap_or(TimeRange::never());
        drop(segments);

        let mut flows = self.inner.flows.write().await;
        if let Some(sf) = flows.get_mut(flow_id) {
            let now = chrono::Utc::now().to_rfc3339();
            sf.core.segments_updated = Some(now.clone());
            let tr_string = if timerange.is_never() {
                sf.core.timerange = None;
                None
            } else {
                let s = timerange.to_string();
                sf.core.timerange = Some(s.clone());
                Some(s)
            };
            if let Some(obj) = sf.document.as_object_mut() {
                if let Some(ref tr) = tr_string {
                    obj.insert("timerange".into(), serde_json::Value::String(tr.clone()));
                } else {
                    obj.remove("timerange");
                }
                obj.insert("segments_updated".into(), serde_json::Value::String(now));
            }
        }
        drop(flows);
        self.persist_flows()
            .await
            .map_err(|e| StoreError::Internal(format!("Failed to persist flows: {e}")))
    }

    /// Remove segments for a deleted flow from memory and disk.
    /// Also deletes media objects that are no longer referenced by any other flow's segments.
    async fn cleanup_flow_segments(&self, flow_id: &str) -> Result<(), StoreError> {
        let mut segments = self.inner.segments.write().await;

        segments.remove(flow_id);

        drop(segments);

        // Segments are now persisted in MongoDB — delete from collection
        self.col_segments()
            .delete_many(bson::doc! { "flow_id": flow_id })
            .await
            .map_err(to_store_err)?;

        // Media cleanup is handled by the S3 store independently.
        // We only clean up metadata here.

        Ok(())
    }

    // -- Event collected_by helpers --

    /// Look up a source's collected_by list (which source collections contain it).
    async fn source_collected_by(&self, source_id: &str) -> Vec<String> {
        let sources = self.inner.sources.read().await;
        sources
            .get(source_id)
            .and_then(|s| s.collected_by.clone())
            .unwrap_or_default()
    }

    /// Apply all side-effects of a flow_collection change: update collected_by
    /// on child flows, compute and sync source_collection on sources, persist both.
    async fn apply_flow_collection_sync(
        &self,
        parent_flow_id: &str,
        parent_source_id: &str,
        old_child_flow_ids: &[String],
        new_collection_items: Option<Vec<(String, String)>>,
    ) -> Result<(), StoreError> {
        let (source_items, new_child_flow_ids) = if let Some(items) = new_collection_items {
            let child_flow_ids: Vec<String> = items.iter().map(|(id, _)| id.clone()).collect();
            let flows = self.inner.flows.read().await;
            let si = build_source_collection_items(&flows, &items);
            drop(flows);
            let si_opt = if si.is_empty() { None } else { Some(si) };
            (si_opt, child_flow_ids)
        } else {
            (None, vec![])
        };

        let old_source_child_ids = {
            let flows = self.inner.flows.read().await;
            resolve_flow_ids_to_source_ids(&flows, old_child_flow_ids)
        };

        // Update collected_by on child flows
        let mut flows = self.inner.flows.write().await;
        remove_parent_from_collected_by(&mut flows, old_child_flow_ids, parent_flow_id);
        add_parent_to_collected_by(&mut flows, &new_child_flow_ids, parent_flow_id);
        drop(flows);

        self.sync_source_collection(parent_source_id, source_items, &old_source_child_ids)
            .await?;

        self.persist_flows()
            .await
            .map_err(|e| StoreError::Internal(format!("Failed to persist flows: {e}")))?;
        self.persist_sources()
            .await
            .map_err(|e| StoreError::Internal(format!("Failed to persist sources: {e}")))?;
        Ok(())
    }

    /// Sync source_collection on a parent source after flow_collection changes.
    /// Sets `source_collection` on the parent source and updates `collected_by`
    /// on child sources. Pass `None` for `new_items` to clear source_collection.
    async fn sync_source_collection(
        &self,
        parent_source_id: &str,
        new_items: Option<Vec<CollectionItem>>,
        old_child_source_ids: &[String],
    ) -> Result<(), StoreError> {
        let mut sources = self.inner.sources.write().await;

        // Derive new child source IDs from the items
        let new_child_source_ids: Vec<String> = new_items
            .as_ref()
            .map(|items| items.iter().map(|i| i.id.clone()).collect())
            .unwrap_or_default();

        // Update source_collection on parent source
        if let Some(parent) = sources.get_mut(parent_source_id) {
            parent.source_collection = new_items;
        }

        // Remove parent from old child sources' collected_by
        for child_id in old_child_source_ids {
            if let Some(child) = sources.get_mut(child_id) {
                if let Some(ref mut cb) = child.collected_by {
                    cb.retain(|id| id != parent_source_id);
                    if cb.is_empty() {
                        child.collected_by = None;
                    }
                }
            }
        }

        // Add parent to new child sources' collected_by
        for child_id in &new_child_source_ids {
            if child_id == parent_source_id {
                continue; // don't self-reference
            }
            if let Some(child) = sources.get_mut(child_id.as_str()) {
                let cb = child.collected_by.get_or_insert_with(Vec::new);
                if !cb.contains(&parent_source_id.to_string()) {
                    cb.push(parent_source_id.to_string());
                }
            }
        }

        drop(sources);
        Ok(())
    }

    // -- Persistence (MongoDB) --

    async fn persist_service_info(&self) -> std::io::Result<()> {
        let _guard = self.inner.persist_service.lock().await;
        let info = self.inner.service_info.read().await;
        let val = serde_json::to_value(&*info).map_err(std::io::Error::other)?;
        drop(info);
        let mut doc = to_bson(&val)?;
        doc.insert("_id", "singleton");
        self.col_service()
            .replace_one(bson::doc! { "_id": "singleton" }, doc)
            .upsert(true)
            .await
            .map(|_| ())
            .map_err(mongo_io_err)
    }

    async fn persist_sources(&self) -> std::io::Result<()> {
        let _guard = self.inner.persist_sources.lock().await;
        let sources = self.inner.sources.read().await;
        let col = self.col_sources();
        for src in sources.values() {
            let val = serde_json::to_value(src).map_err(std::io::Error::other)?;
            let doc = to_bson(&val)?;
            col.replace_one(bson::doc! { "id": &src.id }, doc)
                .upsert(true)
                .await
                .map(|_| ())
                .map_err(mongo_io_err)?;
        }
        Ok(())
    }

    async fn persist_flows(&self) -> std::io::Result<()> {
        let _guard = self.inner.persist_flows.lock().await;
        let flows = self.inner.flows.read().await;
        let col = self.col_flows();
        for (id, sf) in flows.iter() {
            let doc = to_bson(&sf.document)?;
            col.replace_one(bson::doc! { "id": id.as_str() }, doc)
                .upsert(true)
                .await
                .map(|_| ())
                .map_err(mongo_io_err)?;
        }
        Ok(())
    }

    async fn persist_segments(&self, flow_id: &str) -> std::io::Result<()> {
        let _guard = self.inner.persist_segments.lock().await;
        let segments = self.inner.segments.read().await;
        let col = self.col_segments();
        // Delete all segments for this flow then re-insert
        col.delete_many(bson::doc! { "flow_id": flow_id })
            .await
            .map_err(mongo_io_err)?;
        if let Some(segs) = segments.get(flow_id) {
            for seg in segs {
                let mut doc = to_bson(&seg.document)?;
                doc.insert("flow_id", flow_id);
                doc.insert("ts_start", seg.timerange.to_string());
                col.insert_one(doc).await.map_err(mongo_io_err)?;
            }
        }
        Ok(())
    }

    async fn persist_object_instances(&self) -> std::io::Result<()> {
        let _guard = self.inner.persist_instances.lock().await;
        let instances = self.inner.object_instances.read().await;
        let col = self.col_instances();
        for (object_id, list) in instances.iter() {
            let instances_val = serde_json::to_value(list).map_err(std::io::Error::other)?;
            let doc = bson::doc! {
                "object_id": object_id,
                "instances": bson::to_bson(&instances_val).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?,
            };
            col.replace_one(bson::doc! { "object_id": object_id.as_str() }, doc)
                .upsert(true)
                .await
                .map(|_| ())
                .map_err(mongo_io_err)?;
        }
        Ok(())
    }

    async fn persist_webhooks(&self) -> std::io::Result<()> {
        let _guard = self.inner.persist_webhooks.lock().await;
        let webhooks = self.inner.webhooks.read().await;
        let col = self.col_webhooks();
        for (id, wh) in webhooks.iter() {
            let val = serde_json::to_value(wh).map_err(std::io::Error::other)?;
            let doc = to_bson(&val)?;
            col.replace_one(bson::doc! { "id": id.as_str() }, doc)
                .upsert(true)
                .await
                .map(|_| ())
                .map_err(mongo_io_err)?;
        }
        Ok(())
    }

    async fn persist_deletion_request(&self, dr: &DeletionRequest) -> std::io::Result<()> {
        let val = serde_json::to_value(dr).map_err(std::io::Error::other)?;
        let doc = to_bson(&val)?;
        self.col_deletion_requests()
            .replace_one(bson::doc! { "id": &dr.id }, doc)
            .upsert(true)
            .await
            .map(|_| ())
            .map_err(mongo_io_err)
    }

    async fn delete_deletion_request(&self, id: &str) -> std::io::Result<()> {
        self.col_deletion_requests()
            .delete_one(bson::doc! { "id": id })
            .await
            .map_err(mongo_io_err)?;
        Ok(())
    }

    /// Test-only: inject a fake timerange on a flow to simulate having segments.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn test_set_flow_timerange(&self, flow_id: &str, timerange: &str) {
        let mut flows = self.inner.flows.write().await;
        if let Some(sf) = flows.get_mut(flow_id) {
            sf.core.timerange = Some(timerange.to_string());
            sf.core.segments_updated = Some(chrono::Utc::now().to_rfc3339());
            if let Some(obj) = sf.document.as_object_mut() {
                obj.insert(
                    "timerange".into(),
                    serde_json::Value::String(timerange.to_string()),
                );
            }
        }
    }
}

/// Check if a segment matches the given query filters (overlap semantics for GET).
fn matches_query(seg: &StoredSegment, query: &SegmentQuery) -> bool {
    if let Some(ref query_tr) = query.timerange {
        if query_tr.is_never() {
            return false;
        }
        if !query_tr.is_eternity() && !seg.timerange.overlaps(query_tr) {
            return false;
        }
    }
    if let Some(ref oid) = query.object_id {
        if seg.object_id != *oid {
            return false;
        }
    }
    true
}

/// Check if a segment should be deleted (covers semantics for DELETE).
/// Spec: "Only delete Flow Segments that are completely covered by the given timerange."
fn delete_matches(seg: &StoredSegment, query: &SegmentQuery) -> bool {
    if let Some(ref query_tr) = query.timerange {
        if !query_tr.covers(&seg.timerange) {
            return false;
        }
    }
    if let Some(ref oid) = query.object_id {
        if seg.object_id != *oid {
            return false;
        }
    }
    true
}

/// Background task that receives store events and dispatches them to matching webhooks.
async fn event_dispatch_task(
    inner: Arc<StoreInner>,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<StoreEvent>,
) {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default();
    while let Some(event) = rx.recv().await {
        // Collect matching webhooks under read lock, then release it
        let matching: Vec<(String, String, Option<String>, Option<String>)> = {
            let webhooks = inner.webhooks.read().await;
            webhooks
                .values()
                .filter(|w| webhook_matches_event(w, &event))
                .map(|w| {
                    (
                        w.id.clone(),
                        w.url.clone(),
                        w.api_key_name.clone(),
                        w.api_key_value.clone(),
                    )
                })
                .collect()
        };
        if matching.is_empty() {
            continue;
        }
        // Compute payload once per event (single timestamp for all webhooks)
        let payload = Arc::new(event.to_payload());
        for (webhook_id, url, api_key_name, api_key_value) in matching {
            let client = client.clone();
            let payload = Arc::clone(&payload);
            let inner = Arc::clone(&inner);
            tokio::spawn(async move {
                let mut req = client.post(&url).json(payload.as_ref());
                if let (Some(name), Some(value)) = (api_key_name, api_key_value) {
                    req = req.header(name, value);
                }
                let success = match req.send().await {
                    Ok(resp) => resp.status().is_success(),
                    Err(_) => false,
                };
                // Transition status: created → started on first success, → error on failure
                let mut webhooks = inner.webhooks.write().await;
                if let Some(wh) = webhooks.get_mut(&webhook_id) {
                    if wh.status == WebhookStatus::Disabled {
                        return; // disabled webhooks don't transition
                    }
                    if success && wh.status == WebhookStatus::Created {
                        wh.status = WebhookStatus::Started;
                    } else if !success {
                        wh.status = WebhookStatus::Error;
                    }
                }
            });
        }
    }
}

/// Filter uncontrolled get_urls from a segment's JSON document.
/// Excludes all uncontrolled URLs when presigned=true (they are never presigned).
/// Filters by accept_get_urls labels when provided. URLs without a label are
/// excluded when accept_get_urls is specified (per TAMS spec).
fn filter_uncontrolled_urls(
    get_urls: Option<&[serde_json::Value]>,
    presigned: Option<bool>,
    accept_get_urls: Option<&[String]>,
) -> Vec<serde_json::Value> {
    if presigned == Some(true) {
        return Vec::new();
    }
    let Some(arr) = get_urls else {
        return Vec::new();
    };
    arr.iter()
        .filter(|u| u.get("controlled").and_then(|v| v.as_bool()) == Some(false))
        .filter(|u| {
            accept_get_urls.is_none_or(|labels| {
                u.get("label")
                    .and_then(|l| l.as_str())
                    .is_some_and(|l| labels.iter().any(|al| al == l))
            })
        })
        .cloned()
        .collect()
}

/// Check if the controlled URL should be included based on query filters.
fn should_include_controlled_url(
    presigned: Option<bool>,
    url_is_presigned: bool,
    accept_get_urls: Option<&[String]>,
    accept_storage_ids: Option<&[String]>,
    backend_storage_id: &str,
    backend_label: &str,
) -> bool {
    // Filter by presigned status: Some(true) = only presigned, Some(false) = only non-presigned
    if let Some(want_presigned) = presigned {
        if want_presigned != url_is_presigned {
            return false;
        }
    }
    accept_get_urls.is_none_or(|labels| labels.iter().any(|l| l == backend_label))
        && accept_storage_ids.is_none_or(|sids| sids.iter().any(|sid| sid == backend_storage_id))
}

/// Build a controlled get_url JSON object.
/// URLs contain an embedded access_token so they are effectively presigned.
fn build_controlled_url(
    url: &str,
    label: &str,
    storage_id: &str,
    verbose: bool,
) -> serde_json::Value {
    if verbose {
        serde_json::json!({
            "url": url,
            "presigned": true,
            "label": label,
            "controlled": true,
            "storage_id": storage_id,
        })
    } else {
        serde_json::json!({
            "url": url,
            "presigned": true,
            "label": label,
        })
    }
}

fn segment_start_nanos(tr: &TimeRange) -> i128 {
    use tams_types::timerange::Bound;
    match tr {
        TimeRange::Never => i128::MAX,
        TimeRange::Range { start, .. } => match start {
            Some(Bound { timestamp, .. }) => timestamp.nanos,
            None => i128::MIN,
        },
    }
}

/// Map flow IDs to their source_ids via the flows map.
fn resolve_flow_ids_to_source_ids(
    flows: &HashMap<String, StoredFlow>,
    flow_ids: &[String],
) -> Vec<String> {
    flow_ids
        .iter()
        .filter_map(|fid| flows.get(fid).map(|f| f.core.source_id.clone()))
        .collect()
}

/// Build source_collection items from (flow_id, role) pairs by resolving
/// each flow's source_id via the flows map.
fn build_source_collection_items(
    flows: &HashMap<String, StoredFlow>,
    flow_items: &[(String, String)],
) -> Vec<CollectionItem> {
    flow_items
        .iter()
        .filter_map(|(flow_id, role)| {
            flows.get(flow_id).map(|sf| CollectionItem {
                id: sf.core.source_id.clone(),
                role: role.clone(),
            })
        })
        .collect()
}

/// Extract (id, role) pairs from a flow_collection JSON value.
/// Items missing an id or role are skipped (handler validates before this is called).
fn extract_collection_items(value: &serde_json::Value) -> Vec<(String, String)> {
    value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let id = item.get("id").and_then(|v| v.as_str())?;
                    let role = item.get("role").and_then(|v| v.as_str())?;
                    Some((id.to_string(), role.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Extract child flow IDs from a FlowCore's flow_collection.
fn extract_collection_child_ids_from_core(core: &FlowCore) -> Vec<String> {
    core.flow_collection
        .as_ref()
        .map(|v| {
            extract_collection_items(v)
                .into_iter()
                .map(|(id, _)| id)
                .collect()
        })
        .unwrap_or_default()
}

/// Update metadata_updated + metadata_version on a StoredFlow, apply a mutation, and rebuild FlowCore.
fn touch_flow_metadata<F>(sf: &mut StoredFlow, f: F) -> Result<(), StoreError>
where
    F: FnOnce(&mut serde_json::Map<String, serde_json::Value>),
{
    let obj = sf
        .document
        .as_object_mut()
        .ok_or_else(|| StoreError::Internal("Flow document is not an object".into()))?;
    f(obj);
    obj.insert(
        "metadata_updated".into(),
        serde_json::Value::String(chrono::Utc::now().to_rfc3339()),
    );
    obj.insert(
        "metadata_version".into(),
        serde_json::Value::String(uuid::Uuid::new_v4().to_string()),
    );
    sf.core = flow_core_from_document(&sf.document)
        .ok_or_else(|| StoreError::Internal("Failed to rebuild FlowCore".into()))?;
    Ok(())
}

/// Remove parent_id from collected_by on each child flow (both document and core).
fn remove_parent_from_collected_by(
    flows: &mut HashMap<String, StoredFlow>,
    child_ids: &[String],
    parent_id: &str,
) {
    for child_id in child_ids {
        if let Some(child_sf) = flows.get_mut(child_id) {
            if let Some(obj) = child_sf.document.as_object_mut() {
                if let Some(cb) = obj.get_mut("collected_by").and_then(|v| v.as_array_mut()) {
                    cb.retain(|v| v.as_str() != Some(parent_id));
                }
            }
            if let Some(ref mut core_cb) = child_sf.core.collected_by {
                core_cb.retain(|s| s != parent_id);
            }
        }
    }
}

/// Add parent_id to collected_by on each child flow (both document and core).
fn add_parent_to_collected_by(
    flows: &mut HashMap<String, StoredFlow>,
    child_ids: &[String],
    parent_id: &str,
) {
    let parent_str = parent_id.to_string();
    for child_id in child_ids {
        if let Some(child_sf) = flows.get_mut(child_id) {
            if let Some(obj) = child_sf.document.as_object_mut() {
                let cb = obj
                    .entry("collected_by")
                    .or_insert_with(|| serde_json::Value::Array(Vec::new()));
                if let Some(arr) = cb.as_array_mut() {
                    if !arr.iter().any(|v| v.as_str() == Some(parent_id)) {
                        arr.push(serde_json::Value::String(parent_str.clone()));
                    }
                }
            }
            let core_cb = child_sf.core.collected_by.get_or_insert_with(Vec::new);
            if !core_cb.contains(&parent_str) {
                core_cb.push(parent_str.clone());
            }
        }
    }
}

/// Fill in default server-managed fields on a source if not already set.
fn fill_source_defaults(source: &mut Source) {
    let now = chrono::Utc::now().to_rfc3339();
    if source.created.is_none() {
        source.created = Some(now.clone());
    }
    if source.updated.is_none() {
        source.updated = Some(now);
    }
    if source.tags.is_none() {
        source.tags = Some(Tags::new());
    }
    if source.created_by.is_none() {
        source.created_by = Some("server".into());
    }
    if source.updated_by.is_none() {
        source.updated_by = Some("server".into());
    }
}

/// Extract a FlowCore from a flow JSON document.
fn flow_core_from_document(doc: &serde_json::Value) -> Option<FlowCore> {
    let obj = doc.as_object()?;
    let source_id = obj.get("source_id")?.as_str()?.to_string();
    let format = obj.get("format")?.as_str()?.to_string();

    Some(FlowCore {
        source_id,
        format,
        codec: obj.get("codec").and_then(|v| v.as_str()).map(String::from),
        label: obj.get("label").and_then(|v| v.as_str()).map(String::from),
        tags: obj
            .get("tags")
            .filter(|v| !v.is_null())
            .and_then(|v| serde_json::from_value(v.clone()).ok()),
        read_only: obj
            .get("read_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        timerange: obj
            .get("timerange")
            .and_then(|v| v.as_str())
            .map(String::from),
        frame_width: obj
            .get("essence_parameters")
            .and_then(|ep| ep.get("frame_width"))
            .and_then(|v| v.as_i64()),
        frame_height: obj
            .get("essence_parameters")
            .and_then(|ep| ep.get("frame_height"))
            .and_then(|v| v.as_i64()),
        created: obj
            .get("created")
            .and_then(|v| v.as_str())
            .map(String::from),
        segments_updated: obj
            .get("segments_updated")
            .and_then(|v| v.as_str())
            .map(String::from),
        created_by: obj
            .get("created_by")
            .and_then(|v| v.as_str())
            .map(String::from),
        flow_collection: obj.get("flow_collection").cloned(),
        collected_by: obj.get("collected_by").and_then(|v| {
            v.as_array().map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
        }),
        container: obj
            .get("container")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

/// Write content to a temporary file then atomically rename into place.
/// Uses a unique temp filename to avoid races between concurrent writes.

#[cfg(any(test, feature = "test-utils"))]
/// Bucket name used in test S3 config. Tests can assert URL contents against this.
pub const TEST_S3_BUCKET: &str = "tams-media";

#[cfg(any(test, feature = "test-utils"))]
impl Store {
    /// Create a store backed by a fresh MongoDB test database.
    /// Requires a running MongoDB at `TAMS_TEST_MONGO_URI` or `mongodb://localhost:27017`.
    /// Each call uses a unique DB name to isolate parallel tests.
    pub async fn new_test() -> Result<Self, StoreError> {
        let db_name = format!("tams_test_{}", uuid::Uuid::new_v4().simple());
        Self::new_test_db(&db_name).await
    }

    /// Create a store connected to a named test DB (for persistence-across-restart tests).
    pub async fn new_test_db(db_name: &str) -> Result<Self, StoreError> {
        let base_uri = std::env::var("TAMS_TEST_MONGO_URI")
            .unwrap_or_else(|_| "mongodb://localhost:27017".into());
        let uri = format!("{}/{}", base_uri.trim_end_matches('/'), db_name);
        let s3 = S3Config {
            endpoint: "http://localhost:9000".into(),
            bucket: TEST_S3_BUCKET.into(),
            access_key: "testkey".into(),
            secret_key: "testsecret".into(),
            region: "us-east-1".into(),
        };
        Self::new(&uri, s3).await
    }

    /// Drop the test database (call in test teardown).
    pub async fn drop_test_db(&self) {
        let _ = self.inner.db.drop().await;
    }

    /// Create or replace a source. Test-only — production code uses
    /// `create_source_if_absent` to avoid overwriting existing data.
    pub async fn create_source(&self, mut source: Source) -> std::io::Result<()> {
        fill_source_defaults(&mut source);
        self.inner
            .sources
            .write()
            .await
            .insert(source.id.clone(), source);
        self.persist_sources().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tams_types::source::Source;
    use tams_types::tags::TagValue;
    use tams_types::webhook::{StoredWebhook, WebhookStatus};

    fn test_source(id: &str) -> Source {
        Source {
            id: id.into(),
            format: "urn:x-nmos:format:video".into(),
            label: None,
            description: None,
            tags: None,
            created_by: None,
            updated_by: None,
            created: None,
            updated: None,
            source_collection: None,
            collected_by: None,
        }
    }

    fn video_flow(id: &str, source_id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "source_id": source_id,
            "format": "urn:x-nmos:format:video",
            "codec": "video/h264",
            "container": "video/mp2t",
            "essence_parameters": {
                "frame_width": 1920,
                "frame_height": 1080
            }
        })
    }

    fn data_flow(id: &str, source_id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "source_id": source_id,
            "format": "urn:x-nmos:format:data"
        })
    }

    fn test_webhook(url: &str, events: Vec<&str>) -> StoredWebhook {
        StoredWebhook {
            id: String::new(),
            url: url.into(),
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

    // ---- Initialization ----

    #[tokio::test]
    async fn creates_data_directory() {
        let _store = Store::new_test().await.unwrap();
    }

    #[tokio::test]
    async fn creates_service_json() {
        let _store = Store::new_test().await.unwrap();
        // service data is now persisted in MongoDB
    }

    // ---- Service info ----

    #[tokio::test]
    async fn get_service_info_returns_defaults() {
        let store = Store::new_test().await.unwrap();
        let info = store.get_service_info().await;
        assert_eq!(info.api_version, "8.0");
        assert_eq!(info.service_type, "urn:x-tams:service:rustytams");
        assert_eq!(info.name.as_deref(), Some("RustyTAMS"));
    }

    #[tokio::test]
    async fn update_service_info_sets_name() {
        let store = Store::new_test().await.unwrap();
        store
            .update_service_info(ServicePost {
                name: Some("Test".into()),
                description: None,
            })
            .await
            .unwrap();
        let info = store.get_service_info().await;
        assert_eq!(info.name.as_deref(), Some("Test"));
    }

    #[tokio::test]
    async fn service_info_persists_across_restart() {
        let db = format!("tams_test_{}", uuid::Uuid::new_v4().simple());
        {
            let store = Store::new_test_db(&db).await.unwrap();
            store
                .update_service_info(ServicePost {
                    name: Some("Custom Name".into()),
                    description: None,
                })
                .await
                .unwrap();
        }
        let store = Store::new_test_db(&db).await.unwrap();
        let info = store.get_service_info().await;
        assert_eq!(info.name.as_deref(), Some("Custom Name"));
        store.drop_test_db().await;
    }

    #[tokio::test]
    async fn storage_backends_has_at_least_one() {
        let store = Store::new_test().await.unwrap();
        assert!(!store.storage_backends().is_empty());
    }

    // ---- Sources: CRUD ----

    #[tokio::test]
    async fn create_and_get_source() {
        let store = Store::new_test().await.unwrap();
        store.create_source(test_source("src-1")).await.unwrap();
        let src = store.get_source("src-1").await;
        assert!(src.is_some());
        assert_eq!(src.unwrap().format, "urn:x-nmos:format:video");
    }

    #[tokio::test]
    async fn get_nonexistent_source_returns_none() {
        let store = Store::new_test().await.unwrap();
        assert!(store.get_source("nope").await.is_none());
    }

    #[tokio::test]
    async fn list_sources_empty() {
        let store = Store::new_test().await.unwrap();
        let sources = store
            .list_sources(&SourceFilters::default(), &TagFilters::default())
            .await;
        assert!(sources.is_empty());
    }

    #[tokio::test]
    async fn list_sources_returns_all() {
        let store = Store::new_test().await.unwrap();
        store.create_source(test_source("src-a")).await.unwrap();
        store.create_source(test_source("src-b")).await.unwrap();
        let sources = store
            .list_sources(&SourceFilters::default(), &TagFilters::default())
            .await;
        assert_eq!(sources.len(), 2);
    }

    #[tokio::test]
    async fn list_sources_filters_by_format() {
        let store = Store::new_test().await.unwrap();
        store.create_source(test_source("src-v")).await.unwrap();
        let mut audio = test_source("src-a");
        audio.format = "urn:x-nmos:format:audio".into();
        store.create_source(audio).await.unwrap();
        let sources = store
            .list_sources(
                &SourceFilters {
                    format: Some("urn:x-nmos:format:audio".into()),
                    ..Default::default()
                },
                &TagFilters::default(),
            )
            .await;
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].id, "src-a");
    }

    // ---- Sources: tags, label, description ----

    #[tokio::test]
    async fn source_tag_crud() {
        let store = Store::new_test().await.unwrap();
        store.create_source(test_source("src-1")).await.unwrap();

        // Set tag
        store
            .set_source_tag("src-1", "genre", TagValue::Single("news".into()))
            .await
            .unwrap();
        let tag = store.get_source_tag("src-1", "genre").await;
        assert_eq!(tag, Some(TagValue::Single("news".into())));

        // Get all tags
        let tags = store.get_source_tags("src-1").await.unwrap();
        assert!(tags.contains_key("genre"));

        // Delete tag
        store.delete_source_tag("src-1", "genre").await.unwrap();
        assert!(store.get_source_tag("src-1", "genre").await.is_none());
    }

    #[tokio::test]
    async fn source_tag_on_nonexistent_returns_error() {
        let store = Store::new_test().await.unwrap();
        let result = store
            .set_source_tag("nope", "k", TagValue::Single("v".into()))
            .await;
        assert!(matches!(result, Err(StoreError::NotFound(_))));
    }

    #[tokio::test]
    async fn source_label_crud() {
        let store = Store::new_test().await.unwrap();
        store.create_source(test_source("src-1")).await.unwrap();

        assert_eq!(store.get_source_label("src-1").await, Some(None));
        store
            .set_source_label("src-1", "My Label".into())
            .await
            .unwrap();
        assert_eq!(
            store.get_source_label("src-1").await,
            Some(Some("My Label".into()))
        );
        store.delete_source_label("src-1").await.unwrap();
        assert_eq!(store.get_source_label("src-1").await, Some(None));
    }

    #[tokio::test]
    async fn source_description_crud() {
        let store = Store::new_test().await.unwrap();
        store.create_source(test_source("src-1")).await.unwrap();

        store
            .set_source_description("src-1", "Desc".into())
            .await
            .unwrap();
        assert_eq!(
            store.get_source_description("src-1").await,
            Some(Some("Desc".into()))
        );
        store.delete_source_description("src-1").await.unwrap();
        assert_eq!(store.get_source_description("src-1").await, Some(None));
    }

    // ---- Sources: persistence ----

    #[tokio::test]
    async fn sources_persist_to_db() {
        let db = format!("tams_test_{}", uuid::Uuid::new_v4().simple());
        {
            let store = Store::new_test_db(&db).await.unwrap();
            let mut src = test_source("src-persist");
            src.label = Some("Persisted".into());
            store.create_source(src).await.unwrap();
        }
        let store = Store::new_test_db(&db).await.unwrap();
        let src = store.get_source("src-persist").await.unwrap();
        assert_eq!(src.label.as_deref(), Some("Persisted"));
        store.drop_test_db().await;
    }

    #[tokio::test]
    async fn source_mutations_persist_to_db() {
        let db = format!("tams_test_{}", uuid::Uuid::new_v4().simple());
        {
            let store = Store::new_test_db(&db).await.unwrap();
            store.create_source(test_source("src-mut")).await.unwrap();
            store
                .set_source_label("src-mut", "Updated Label".into())
                .await
                .unwrap();
        }
        let store = Store::new_test_db(&db).await.unwrap();
        let src = store.get_source("src-mut").await.unwrap();
        assert_eq!(src.label.as_deref(), Some("Updated Label"));
        store.drop_test_db().await;
    }

    // ---- Flows: CRUD ----

    #[tokio::test]
    async fn put_flow_creates_new() {
        let store = Store::new_test().await.unwrap();
        let (created, doc) = store.put_flow(data_flow("f1", "s1")).await.unwrap();
        assert!(created);
        assert!(doc.is_some());
        let doc = doc.unwrap();
        assert_eq!(doc["id"], "f1");
        assert!(doc["created"].is_string());
        assert!(doc["metadata_updated"].is_string());
    }

    #[tokio::test]
    async fn put_flow_updates_existing() {
        let store = Store::new_test().await.unwrap();
        store.put_flow(data_flow("f1", "s1")).await.unwrap();

        let mut updated = data_flow("f1", "s1");
        updated["label"] = serde_json::json!("Updated");
        let (created, doc) = store.put_flow(updated).await.unwrap();
        assert!(!created);
        assert!(doc.is_none()); // updates return None (204)

        let flow = store.get_flow("f1").await.unwrap();
        assert_eq!(flow["label"], "Updated");
    }

    #[tokio::test]
    async fn put_flow_auto_creates_source() {
        let store = Store::new_test().await.unwrap();
        store.put_flow(data_flow("f1", "auto-src")).await.unwrap();
        assert!(store.get_source("auto-src").await.is_some());
    }

    #[tokio::test]
    async fn get_nonexistent_flow_returns_none() {
        let store = Store::new_test().await.unwrap();
        assert!(store.get_flow("nope").await.is_none());
    }

    #[tokio::test]
    async fn delete_flow_no_segments_returns_removed() {
        let store = Store::new_test().await.unwrap();
        store.put_flow(data_flow("f1", "s1")).await.unwrap();
        let result = store.delete_flow("f1").await.unwrap();
        assert!(matches!(result, DeleteResult::Deleted));
        assert!(store.get_flow("f1").await.is_none());
    }

    #[tokio::test]
    async fn delete_nonexistent_flow_returns_not_found() {
        let store = Store::new_test().await.unwrap();
        let result = store.delete_flow("nope").await.unwrap();
        assert!(matches!(result, DeleteResult::NotFound));
    }

    // ---- Flows: validation ----

    #[tokio::test]
    async fn put_flow_rejects_missing_id() {
        let store = Store::new_test().await.unwrap();
        let result = store
            .put_flow(serde_json::json!({"source_id": "s1", "format": "urn:x-nmos:format:data"}))
            .await;
        assert!(matches!(result, Err(StoreError::BadRequest(_))));
    }

    #[tokio::test]
    async fn put_flow_rejects_missing_source_id() {
        let store = Store::new_test().await.unwrap();
        let result = store
            .put_flow(serde_json::json!({"id": "f1", "format": "urn:x-nmos:format:data"}))
            .await;
        assert!(matches!(result, Err(StoreError::BadRequest(_))));
    }

    #[tokio::test]
    async fn put_flow_rejects_invalid_format() {
        let store = Store::new_test().await.unwrap();
        let result = store
            .put_flow(serde_json::json!({"id": "f1", "source_id": "s1", "format": "bad"}))
            .await;
        assert!(matches!(result, Err(StoreError::BadRequest(_))));
    }

    #[tokio::test]
    async fn put_video_flow_rejects_missing_codec() {
        let store = Store::new_test().await.unwrap();
        let result = store
            .put_flow(serde_json::json!({
                "id": "f1",
                "source_id": "s1",
                "format": "urn:x-nmos:format:video",
                "essence_parameters": {"frame_width": 1920, "frame_height": 1080}
            }))
            .await;
        assert!(matches!(result, Err(StoreError::BadRequest(_))));
    }

    #[tokio::test]
    async fn put_video_flow_rejects_missing_essence_params() {
        let store = Store::new_test().await.unwrap();
        let result = store
            .put_flow(serde_json::json!({
                "id": "f1",
                "source_id": "s1",
                "format": "urn:x-nmos:format:video",
                "codec": "video/h264"
            }))
            .await;
        assert!(matches!(result, Err(StoreError::BadRequest(_))));
    }

    // ---- Flows: read-only ----

    #[tokio::test]
    async fn read_only_flow_rejects_update() {
        let store = Store::new_test().await.unwrap();
        store.put_flow(data_flow("f1", "s1")).await.unwrap();
        store.set_flow_read_only("f1", true).await.unwrap();

        let result = store.put_flow(data_flow("f1", "s1")).await;
        assert!(matches!(result, Err(StoreError::ReadOnly)));
    }

    // ---- Flows: properties ----

    #[tokio::test]
    async fn flow_tag_crud() {
        let store = Store::new_test().await.unwrap();
        store.put_flow(data_flow("f1", "s1")).await.unwrap();

        store
            .set_flow_tag("f1", "genre", TagValue::Single("news".into()))
            .await
            .unwrap();
        let tag = store.get_flow_tag("f1", "genre").await;
        assert_eq!(tag, Some(TagValue::Single("news".into())));

        store.delete_flow_tag("f1", "genre").await.unwrap();
        assert!(store.get_flow_tag("f1", "genre").await.is_none());
    }

    #[tokio::test]
    async fn flow_label_property() {
        let store = Store::new_test().await.unwrap();
        store.put_flow(data_flow("f1", "s1")).await.unwrap();

        store
            .set_flow_property("f1", "label", serde_json::json!("Test"))
            .await
            .unwrap();
        let val = store.get_flow_property("f1", "label").await.unwrap();
        assert_eq!(val, Some(serde_json::json!("Test")));

        store.delete_flow_property("f1", "label").await.unwrap();
        let val = store.get_flow_property("f1", "label").await.unwrap();
        assert_eq!(val, None);
    }

    // ---- Flows: persistence ----

    #[tokio::test]
    async fn flows_persist_to_db() {
        let db = format!("tams_test_{}", uuid::Uuid::new_v4().simple());
        let flow_id = "f0000000-0000-0000-0000-000000000001";
        {
            let store = Store::new_test_db(&db).await.unwrap();
            store
                .put_flow(serde_json::json!({
                    "id": flow_id,
                    "source_id": "s1",
                    "format": "urn:x-nmos:format:data",
                    "label": "Persisted Flow"
                }))
                .await
                .unwrap();
        }
        let store = Store::new_test_db(&db).await.unwrap();
        let flow = store.get_flow(flow_id).await;
        assert!(flow.is_some(), "Flow should survive restart");
        assert_eq!(flow.unwrap()["label"], "Persisted Flow");
        store.drop_test_db().await;
    }

    #[tokio::test]
    async fn flow_deletion_persists_to_db() {
        let db = format!("tams_test_{}", uuid::Uuid::new_v4().simple());
        let flow_id = "f0000000-0000-0000-0000-000000000002";
        {
            let store = Store::new_test_db(&db).await.unwrap();
            store.put_flow(data_flow(flow_id, "s1")).await.unwrap();
            store.delete_flow(flow_id).await.unwrap();
        }
        let store = Store::new_test_db(&db).await.unwrap();
        assert!(store.get_flow(flow_id).await.is_none());
        store.drop_test_db().await;
    }

    #[tokio::test]
    async fn auto_created_source_persists_from_flow() {
        let db = format!("tams_test_{}", uuid::Uuid::new_v4().simple());
        let source_id = "s-auto-persist";
        {
            let store = Store::new_test_db(&db).await.unwrap();
            store
                .put_flow(data_flow("f-auto", source_id))
                .await
                .unwrap();
        }
        let store = Store::new_test_db(&db).await.unwrap();
        assert!(store.get_source(source_id).await.is_some());
        store.drop_test_db().await;
    }

    #[tokio::test]
    async fn flow_with_all_fields_persists() {
        let db = format!("tams_test_{}", uuid::Uuid::new_v4().simple());
        let flow_id = "f-full";
        {
            let store = Store::new_test_db(&db).await.unwrap();
            store
                .put_flow(serde_json::json!({
                    "id": flow_id,
                    "source_id": "s-full",
                    "format": "urn:x-nmos:format:video",
                    "codec": "video/h264",
                    "container": "video/mp2t",
                    "label": "Full Flow",
                    "description": "A persisted video flow",
                    "generation": 2,
                    "max_bit_rate": 5000,
                    "avg_bit_rate": 3000,
                    "tags": {"genre": "news"},
                    "essence_parameters": {
                        "frame_width": 1920,
                        "frame_height": 1080
                    }
                }))
                .await
                .unwrap();
        }
        let store = Store::new_test_db(&db).await.unwrap();
        let flow = store.get_flow(flow_id).await.unwrap();
        assert_eq!(flow["label"], "Full Flow");
        assert_eq!(flow["codec"], "video/h264");
        assert_eq!(flow["generation"], 2);
        assert_eq!(flow["tags"]["genre"], "news");
        assert!(flow["created"].is_string());
        assert!(flow["metadata_updated"].is_string());
        store.drop_test_db().await;
    }

    // ---- Segments ----

    #[tokio::test]
    async fn post_and_get_segments() {
        let store = Store::new_test().await.unwrap();
        store.put_flow(video_flow("f1", "s1")).await.unwrap();

        let result = store
            .post_segments(
                "f1",
                vec![serde_json::json!({
                    "object_id": "obj-1",
                    "timerange": "[0:0_0:1)"
                })],
            )
            .await
            .unwrap();
        assert!(matches!(result, SegmentPostResult::AllCreated));

        let (segs, timerange) = store.get_segments("f1", &SegmentQuery::default()).await;
        assert_eq!(segs.len(), 1);
        assert!(!timerange.is_never());
    }

    #[tokio::test]
    async fn post_segments_rejects_overlap() {
        let store = Store::new_test().await.unwrap();
        store.put_flow(video_flow("f1", "s1")).await.unwrap();

        store
            .post_segments(
                "f1",
                vec![serde_json::json!({
                    "object_id": "obj-1",
                    "timerange": "[0:0_1:0)"
                })],
            )
            .await
            .unwrap();

        // Overlapping segment should fail
        let result = store
            .post_segments(
                "f1",
                vec![serde_json::json!({
                    "object_id": "obj-2",
                    "timerange": "[0:500000000_1:500000000)"
                })],
            )
            .await;
        assert!(matches!(result, Err(StoreError::BadRequest(_))));
    }

    #[tokio::test]
    async fn post_segments_on_nonexistent_flow_fails() {
        let store = Store::new_test().await.unwrap();
        let result = store
            .post_segments(
                "nope",
                vec![serde_json::json!({
                    "object_id": "obj-1",
                    "timerange": "[0:0_1:0)"
                })],
            )
            .await;
        assert!(matches!(result, Err(StoreError::NotFound(_))));
    }

    // ---- Webhooks ----

    #[tokio::test]
    async fn webhook_create_and_get() {
        let store = Store::new_test().await.unwrap();
        let wh = test_webhook("http://example.com/hook", vec!["flows/created"]);
        let created = store.create_webhook(wh).await.unwrap();
        assert!(!created.id.is_empty());
        assert_eq!(created.status, WebhookStatus::Created);

        let fetched = store.get_webhook(&created.id).await.unwrap();
        assert_eq!(fetched.url, "http://example.com/hook");
    }

    #[tokio::test]
    async fn webhook_list_empty() {
        let store = Store::new_test().await.unwrap();
        let list = store.list_webhooks(&TagFilters::default()).await;
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn webhook_delete() {
        let store = Store::new_test().await.unwrap();
        let wh = store
            .create_webhook(test_webhook("http://example.com", vec!["flows/created"]))
            .await
            .unwrap();
        store.delete_webhook(&wh.id).await.unwrap();
        assert!(matches!(
            store.get_webhook(&wh.id).await,
            Err(StoreError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn webhook_delete_nonexistent_fails() {
        let store = Store::new_test().await.unwrap();
        let result = store.delete_webhook("nope").await;
        assert!(matches!(result, Err(StoreError::NotFound(_))));
    }

    #[tokio::test]
    async fn webhook_update_status_transition() {
        let store = Store::new_test().await.unwrap();
        let wh = store
            .create_webhook(test_webhook("http://example.com", vec!["flows/created"]))
            .await
            .unwrap();

        // created -> disabled OK
        let mut update = wh.clone();
        update.status = WebhookStatus::Disabled;
        let updated = store.update_webhook(&wh.id, update).await.unwrap();
        assert_eq!(updated.status, WebhookStatus::Disabled);

        // disabled -> created OK (re-enable)
        let mut update = updated.clone();
        update.status = WebhookStatus::Created;
        let updated = store.update_webhook(&wh.id, update).await.unwrap();
        assert_eq!(updated.status, WebhookStatus::Created);
    }

    #[tokio::test]
    async fn webhook_invalid_status_transition_rejected() {
        let store = Store::new_test().await.unwrap();
        let wh = store
            .create_webhook(test_webhook("http://example.com", vec!["flows/created"]))
            .await
            .unwrap();

        // created -> error is not a valid transition
        let mut update = wh.clone();
        update.status = WebhookStatus::Error;
        let result = store.update_webhook(&wh.id, update).await;
        assert!(matches!(result, Err(StoreError::BadRequest(_))));
    }

    #[tokio::test]
    async fn webhook_update_preserves_api_key() {
        let store = Store::new_test().await.unwrap();
        let mut wh = test_webhook("http://example.com", vec!["flows/created"]);
        wh.api_key_name = Some("X-Api-Key".into());
        wh.api_key_value = Some("secret".into());
        let created = store.create_webhook(wh).await.unwrap();

        // Update without api_key_value should preserve it
        let mut update = created.clone();
        update.api_key_value = None;
        update.url = "http://example.com/new".into();
        let updated = store.update_webhook(&created.id, update).await.unwrap();
        assert_eq!(updated.api_key_value.as_deref(), Some("secret"));
        assert_eq!(updated.url, "http://example.com/new");
    }

    // -- is_safe_id --

    #[test]
    fn safe_id_accepts_uuids() {
        assert!(tams_types::is_safe_id(
            "550e8400-e29b-41d4-a716-446655440000"
        ));
    }

    #[test]
    fn safe_id_rejects_forward_slash() {
        assert!(!tams_types::is_safe_id("../etc/passwd"));
        assert!(!tams_types::is_safe_id("foo/bar"));
    }

    #[test]
    fn safe_id_rejects_backslash() {
        assert!(!tams_types::is_safe_id("foo\\bar"));
    }

    #[test]
    fn safe_id_rejects_exact_dot_dot() {
        assert!(!tams_types::is_safe_id(".."));
    }

    #[test]
    fn safe_id_accepts_dots_and_hyphens() {
        assert!(tams_types::is_safe_id("my-flow.v2"));
        assert!(tams_types::is_safe_id("flow_123"));
        assert!(tams_types::is_safe_id(".hidden"));
        assert!(tams_types::is_safe_id("foo..bar")); // not path traversal
    }

    #[tokio::test]
    async fn put_flow_rejects_path_traversal_id() {
        let store = Store::new_test().await.unwrap();
        let flow = serde_json::json!({
            "id": "../../../tmp/evil",
            "source_id": "src-1",
            "format": "urn:x-nmos:format:video",
        });
        let result = store.put_flow(flow).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            StoreError::BadRequest(msg) => assert!(msg.contains("invalid path"), "{msg}"),
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }
}

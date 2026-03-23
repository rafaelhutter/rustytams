use salvo::http::StatusCode;
use salvo::prelude::*;

use crate::error::AppError;
use crate::extract::tag_filters_from_request;
use crate::extract::{paginate_and_set_headers, pagination_from_request};
use crate::handlers::get_store;
use tams_types::webhook::{StoredWebhook, WebhookStatus, VALID_EVENT_TYPES};

/// Strip api_key_value from a webhook before returning it in a response.
fn webhook_to_response(webhook: &StoredWebhook) -> serde_json::Value {
    let mut cleaned = webhook.clone();
    cleaned.api_key_value = None;
    serde_json::to_value(cleaned).unwrap()
}

fn parse_string_array(body: &serde_json::Value, field: &str) -> Option<Vec<String>> {
    body.get(field).and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    })
}

/// Parse status from JSON body. Returns Ok(status) or Err(message).
fn parse_status(body: &serde_json::Value, required: bool) -> Result<WebhookStatus, String> {
    match body.get("status").and_then(|v| v.as_str()) {
        Some("created") => Ok(WebhookStatus::Created),
        Some("disabled") => Ok(WebhookStatus::Disabled),
        Some(other) => Err(format!(
            "Client can only set status to 'created' or 'disabled', got '{other}'"
        )),
        None if required => Err("Missing required field: status".into()),
        None => Ok(WebhookStatus::Created),
    }
}

/// Validate that all event types are recognized.
fn validate_event_types(events: &[String]) -> Result<(), String> {
    for e in events {
        if !VALID_EVENT_TYPES.contains(&e.as_str()) {
            return Err(format!("Invalid event type: {e}"));
        }
    }
    Ok(())
}

/// Extract common webhook fields from a JSON body into a StoredWebhook.
/// `url` and `events` are required for POST, optional for PUT (caller handles defaults).
fn extract_webhook_fields(
    body: &serde_json::Value,
    status: WebhookStatus,
    url: String,
    events: Vec<String>,
) -> StoredWebhook {
    StoredWebhook {
        id: String::new(),
        url,
        events,
        api_key_name: body
            .get("api_key_name")
            .and_then(|v| v.as_str())
            .map(String::from),
        api_key_value: body
            .get("api_key_value")
            .and_then(|v| v.as_str())
            .map(String::from),
        flow_ids: parse_string_array(body, "flow_ids"),
        source_ids: parse_string_array(body, "source_ids"),
        flow_collected_by_ids: parse_string_array(body, "flow_collected_by_ids"),
        source_collected_by_ids: parse_string_array(body, "source_collected_by_ids"),
        accept_get_urls: parse_string_array(body, "accept_get_urls"),
        accept_storage_ids: parse_string_array(body, "accept_storage_ids"),
        presigned: body.get("presigned").and_then(|v| v.as_bool()),
        verbose_storage: body.get("verbose_storage").and_then(|v| v.as_bool()),
        tags: body
            .get("tags")
            .and_then(|v| serde_json::from_value(v.clone()).ok()),
        status,
        error: None,
    }
}

fn parse_webhook_from_body(body: &serde_json::Value) -> Result<StoredWebhook, String> {
    let url = body
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: url")?
        .to_string();

    let events = body
        .get("events")
        .and_then(|v| v.as_array())
        .ok_or("Missing required field: events")?;
    let events: Vec<String> = events
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    validate_event_types(&events)?;

    let status = parse_status(body, false)?;
    Ok(extract_webhook_fields(body, status, url, events))
}

/// Extract webhookId path parameter.
fn webhook_id(req: &Request) -> String {
    req.param::<String>("webhookId")
        .expect("webhookId path param missing from route")
}

/// GET /service/webhooks
#[handler]
pub async fn get_webhooks(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    if !crate::extract::validate_query_params(req, &["limit", "page"], res) {
        return;
    }
    let store = get_store(depot);
    let tag_filters = tag_filters_from_request(req);
    let webhooks = store.list_webhooks(&tag_filters).await;

    let pagination = pagination_from_request(req);
    let page = paginate_and_set_headers(&webhooks, &pagination, req, res);

    let response: Vec<serde_json::Value> = page.iter().map(webhook_to_response).collect();
    res.render(Json(response));
}

/// POST /service/webhooks
#[handler]
pub async fn post_webhook(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let body: serde_json::Value = match crate::handlers::parse_json(req, res).await {
        Some(v) => v,
        None => return,
    };

    let webhook = match parse_webhook_from_body(&body) {
        Ok(w) => w,
        Err(msg) => {
            crate::error::AppError::bad_request(&msg).write_to(res);
            return;
        }
    };

    match store.create_webhook(webhook).await {
        Ok(created) => {
            res.status_code(StatusCode::CREATED);
            res.render(Json(webhook_to_response(&created)));
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

/// GET /service/webhooks/{webhookId}
#[handler]
pub async fn get_webhook(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = webhook_id(req);

    match store.get_webhook(&id).await {
        Ok(webhook) => res.render(Json(webhook_to_response(&webhook))),
        Err(e) => AppError::from(e).write_to(res),
    }
}

/// PUT /service/webhooks/{webhookId}
#[handler]
pub async fn put_webhook(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = webhook_id(req);

    let body: serde_json::Value = match crate::handlers::parse_json(req, res).await {
        Some(v) => v,
        None => return,
    };

    // Status is required for PUT
    let status = match parse_status(&body, true) {
        Ok(s) => s,
        Err(msg) => {
            crate::error::AppError::bad_request(msg).write_to(res);
            return;
        }
    };

    // URL is required for PUT (full replacement)
    let url = match body.get("url").and_then(|v| v.as_str()) {
        Some(u) => u.to_string(),
        None => {
            crate::error::AppError::bad_request("Missing required field: url").write_to(res);
            return;
        }
    };

    // Events required for PUT (full replacement)
    let events: Vec<String> = match body.get("events").and_then(|v| v.as_array()) {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        None => {
            crate::error::AppError::bad_request("Missing required field: events").write_to(res);
            return;
        }
    };
    if let Err(msg) = validate_event_types(&events) {
        crate::error::AppError::bad_request(msg).write_to(res);
        return;
    }

    let mut update = extract_webhook_fields(&body, status, url, events);
    update.id = id.clone();

    match store.update_webhook(&id, update).await {
        Ok(updated) => {
            res.status_code(StatusCode::CREATED);
            res.render(Json(webhook_to_response(&updated)));
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

/// DELETE /service/webhooks/{webhookId}
#[handler]
pub async fn delete_webhook(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = webhook_id(req);

    match store.delete_webhook(&id).await {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

#[cfg(test)]
mod tests {
    use salvo::prelude::*;
    use salvo::test::{ResponseExt, TestClient};

    use crate::test_utils::test_service;

    fn webhook_body() -> serde_json::Value {
        serde_json::json!({
            "url": "http://example.com/webhook",
            "events": ["flows/created", "flows/updated"]
        })
    }

    /// POST a webhook and return the response body.
    async fn create_webhook(service: &Service) -> serde_json::Value {
        let mut resp = TestClient::post("http://localhost:5800/service/webhooks")
            .basic_auth("test", Some("password"))
            .json(&webhook_body())
            .send(service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201);
        resp.take_json().await.unwrap()
    }

    // -- GET /service/webhooks --

    #[tokio::test]
    async fn get_webhooks_empty() {
        let (service, _store, _tmp) = test_service().await;
        let mut resp = TestClient::get("http://localhost:5800/service/webhooks")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn get_webhooks_returns_created() {
        let (service, _store, _tmp) = test_service().await;
        create_webhook(&service).await;

        let mut resp = TestClient::get("http://localhost:5800/service/webhooks")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn head_webhooks_no_body() {
        let (service, _store, _tmp) = test_service().await;
        let mut resp = TestClient::head("http://localhost:5800/service/webhooks")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body = resp.take_string().await.unwrap();
        assert!(body.is_empty());
    }

    // -- POST /service/webhooks --

    #[tokio::test]
    async fn post_webhook_returns_201_with_id() {
        let (service, _store, _tmp) = test_service().await;
        let body = create_webhook(&service).await;
        assert!(body.get("id").is_some());
        assert_eq!(body["status"], "created");
        assert_eq!(body["url"], "http://example.com/webhook");
    }

    #[tokio::test]
    async fn post_webhook_missing_url_returns_400() {
        let (service, _store, _tmp) = test_service().await;
        let resp = TestClient::post("http://localhost:5800/service/webhooks")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"events": ["flows/created"]}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 400);
    }

    #[tokio::test]
    async fn post_webhook_missing_events_returns_400() {
        let (service, _store, _tmp) = test_service().await;
        let resp = TestClient::post("http://localhost:5800/service/webhooks")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"url": "http://example.com/hook"}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 400);
    }

    #[tokio::test]
    async fn post_webhook_invalid_event_type_returns_400() {
        let (service, _store, _tmp) = test_service().await;
        let resp = TestClient::post("http://localhost:5800/service/webhooks")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "url": "http://example.com/hook",
                "events": ["invalid/event"]
            }))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 400);
    }

    #[tokio::test]
    async fn post_webhook_api_key_value_not_returned() {
        let (service, _store, _tmp) = test_service().await;
        let mut resp = TestClient::post("http://localhost:5800/service/webhooks")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "url": "http://example.com/hook",
                "events": ["flows/created"],
                "api_key_name": "X-Api-Key",
                "api_key_value": "secret123"
            }))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["api_key_name"], "X-Api-Key");
        assert!(body.get("api_key_value").is_none());
    }

    #[tokio::test]
    async fn post_webhook_with_disabled_status() {
        let (service, _store, _tmp) = test_service().await;
        let mut resp = TestClient::post("http://localhost:5800/service/webhooks")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "url": "http://example.com/hook",
                "events": ["flows/created"],
                "status": "disabled"
            }))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["status"], "disabled");
    }

    // -- GET /service/webhooks/{webhookId} --

    #[tokio::test]
    async fn get_webhook_by_id() {
        let (service, _store, _tmp) = test_service().await;
        let created = create_webhook(&service).await;
        let id = created["id"].as_str().unwrap();

        let mut resp = TestClient::get(format!("http://localhost:5800/service/webhooks/{id}"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["id"], id);
        assert_eq!(body["url"], "http://example.com/webhook");
    }

    #[tokio::test]
    async fn get_webhook_not_found() {
        let (service, _store, _tmp) = test_service().await;
        let resp = TestClient::get("http://localhost:5800/service/webhooks/nonexistent")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 404);
    }

    #[tokio::test]
    async fn head_webhook_no_body() {
        let (service, _store, _tmp) = test_service().await;
        let created = create_webhook(&service).await;
        let id = created["id"].as_str().unwrap();

        let mut resp = TestClient::head(format!("http://localhost:5800/service/webhooks/{id}"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body = resp.take_string().await.unwrap();
        assert!(body.is_empty());
    }

    // -- PUT /service/webhooks/{webhookId} --

    #[tokio::test]
    async fn put_webhook_updates() {
        let (service, _store, _tmp) = test_service().await;
        let created = create_webhook(&service).await;
        let id = created["id"].as_str().unwrap();

        let mut resp = TestClient::put(format!("http://localhost:5800/service/webhooks/{id}"))
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "url": "http://example.com/updated",
                "events": ["flows/deleted"],
                "status": "created"
            }))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["url"], "http://example.com/updated");
        assert_eq!(body["events"].as_array().unwrap()[0], "flows/deleted");
    }

    #[tokio::test]
    async fn put_webhook_disable() {
        let (service, _store, _tmp) = test_service().await;
        let created = create_webhook(&service).await;
        let id = created["id"].as_str().unwrap();

        let mut resp = TestClient::put(format!("http://localhost:5800/service/webhooks/{id}"))
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "url": "http://example.com/webhook",
                "events": ["flows/created"],
                "status": "disabled"
            }))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["status"], "disabled");
    }

    #[tokio::test]
    async fn put_webhook_reenable_from_disabled() {
        let (service, _store, _tmp) = test_service().await;
        let created = create_webhook(&service).await;
        let id = created["id"].as_str().unwrap();

        // Disable first
        TestClient::put(format!("http://localhost:5800/service/webhooks/{id}"))
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "url": "http://example.com/webhook",
                "events": ["flows/created"],
                "status": "disabled"
            }))
            .send(&service)
            .await;

        // Re-enable
        let mut resp = TestClient::put(format!("http://localhost:5800/service/webhooks/{id}"))
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "url": "http://example.com/webhook",
                "events": ["flows/created"],
                "status": "created"
            }))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["status"], "created");
    }

    #[tokio::test]
    async fn put_webhook_not_found() {
        let (service, _store, _tmp) = test_service().await;
        let resp = TestClient::put("http://localhost:5800/service/webhooks/nonexistent")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "url": "http://example.com/hook",
                "events": ["flows/created"],
                "status": "created"
            }))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 404);
    }

    #[tokio::test]
    async fn put_webhook_missing_url_returns_400() {
        let (service, _store, _tmp) = test_service().await;
        let created = create_webhook(&service).await;
        let id = created["id"].as_str().unwrap();

        let resp = TestClient::put(format!("http://localhost:5800/service/webhooks/{id}"))
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "events": ["flows/created"],
                "status": "created"
            }))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 400);
    }

    #[tokio::test]
    async fn put_webhook_missing_events_returns_400() {
        let (service, _store, _tmp) = test_service().await;
        let created = create_webhook(&service).await;
        let id = created["id"].as_str().unwrap();

        let resp = TestClient::put(format!("http://localhost:5800/service/webhooks/{id}"))
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "url": "http://example.com/hook",
                "status": "created"
            }))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 400);
    }

    // -- DELETE /service/webhooks/{webhookId} --

    #[tokio::test]
    async fn delete_webhook_returns_204() {
        let (service, _store, _tmp) = test_service().await;
        let created = create_webhook(&service).await;
        let id = created["id"].as_str().unwrap();

        let resp = TestClient::delete(format!("http://localhost:5800/service/webhooks/{id}"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 204);

        // Verify it's gone
        let resp = TestClient::get(format!("http://localhost:5800/service/webhooks/{id}"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 404);
    }

    #[tokio::test]
    async fn delete_webhook_not_found() {
        let (service, _store, _tmp) = test_service().await;
        let resp = TestClient::delete("http://localhost:5800/service/webhooks/nonexistent")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 404);
    }

    // -- Tag filtering --

    #[tokio::test]
    async fn get_webhooks_tag_filter() {
        let (service, _store, _tmp) = test_service().await;

        // Create webhook with tags
        TestClient::post("http://localhost:5800/service/webhooks")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "url": "http://example.com/hook1",
                "events": ["flows/created"],
                "tags": {"env": "prod"}
            }))
            .send(&service)
            .await;

        // Create webhook without matching tag
        TestClient::post("http://localhost:5800/service/webhooks")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "url": "http://example.com/hook2",
                "events": ["flows/created"],
                "tags": {"env": "dev"}
            }))
            .send(&service)
            .await;

        let mut resp = TestClient::get("http://localhost:5800/service/webhooks?tag.env=prod")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let webhooks = body.as_array().unwrap();
        assert_eq!(webhooks.len(), 1);
        assert_eq!(webhooks[0]["url"], "http://example.com/hook1");
    }

    // -- Pagination --

    #[tokio::test]
    async fn get_webhooks_pagination() {
        let (service, _store, _tmp) = test_service().await;

        // Create 3 webhooks
        for i in 0..3 {
            TestClient::post("http://localhost:5800/service/webhooks")
                .basic_auth("test", Some("password"))
                .json(&serde_json::json!({
                    "url": format!("http://example.com/hook{i}"),
                    "events": ["flows/created"]
                }))
                .send(&service)
                .await;
        }

        let mut resp = TestClient::get("http://localhost:5800/service/webhooks?limit=2")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body.as_array().unwrap().len(), 2);
        assert!(resp.headers().get("x-paging-limit").is_some());
        assert!(resp.headers().get("link").is_some());
    }

    // -- Webhook with filters --

    #[tokio::test]
    async fn post_webhook_with_flow_ids_filter() {
        let (service, _store, _tmp) = test_service().await;
        let mut resp = TestClient::post("http://localhost:5800/service/webhooks")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "url": "http://example.com/hook",
                "events": ["flows/created"],
                "flow_ids": ["flow-1", "flow-2"]
            }))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let flow_ids = body["flow_ids"].as_array().unwrap();
        assert_eq!(flow_ids.len(), 2);
    }

    #[tokio::test]
    async fn post_webhook_with_source_ids_filter() {
        let (service, _store, _tmp) = test_service().await;
        let mut resp = TestClient::post("http://localhost:5800/service/webhooks")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "url": "http://example.com/hook",
                "events": ["sources/created"],
                "source_ids": ["src-1"]
            }))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["source_ids"].as_array().unwrap().len(), 1);
    }

    // -- End-to-end event dispatch --

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_flow_created_dispatches_to_webhook() {
        use tokio::io::AsyncReadExt;

        let (service, _store, _tmp) = test_service().await;

        // Bind a TCP listener on a random port to receive the webhook POST
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let webhook_url = format!("http://127.0.0.1:{}/webhook", addr.port());

        // Spawn a task to accept one connection and capture the HTTP body
        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).await.unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            // Send a minimal 200 response so reqwest doesn't error
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
            tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes())
                .await
                .unwrap();
            let _ = tx.send(request);
        });

        // Register a webhook pointing to our listener
        TestClient::post("http://localhost:5800/service/webhooks")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "url": webhook_url,
                "events": ["flows/created"],
                "api_key_name": "X-Hook-Key",
                "api_key_value": "secret-token"
            }))
            .send(&service)
            .await;

        // Create a flow — this should trigger the webhook
        let flow_id = uuid::Uuid::new_v4().to_string();
        let resp = TestClient::put(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "id": &flow_id,
                "source_id": uuid::Uuid::new_v4().to_string(),
                "format": "urn:x-nmos:format:video",
                "codec": "video/h264",
                "essence_parameters": {
                    "frame_rate": {"numerator": 25, "denominator": 1},
                    "frame_width": 1920,
                    "frame_height": 1080
                }
            }))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201, "Flow PUT must succeed");

        // Wait for the webhook POST (with timeout)
        let request = tokio::time::timeout(std::time::Duration::from_secs(5), rx)
            .await
            .expect("Timed out waiting for webhook POST")
            .expect("Webhook receiver task failed");

        // Verify it's a POST to /webhook with our API key header
        assert!(
            request.starts_with("POST /webhook"),
            "Expected POST, got: {}",
            &request[..40.min(request.len())]
        );
        assert!(
            request.contains("x-hook-key: secret-token")
                || request.contains("X-Hook-Key: secret-token"),
            "Expected X-Hook-Key header in request"
        );

        // Extract JSON body (after the blank line in HTTP request)
        let body_start = request.find("\r\n\r\n").expect("No HTTP body separator") + 4;
        let body_str = &request[body_start..];
        let payload: serde_json::Value =
            serde_json::from_str(body_str).expect("Webhook body is not valid JSON");

        assert_eq!(payload["event_type"], "flows/created");
        assert!(payload["event_timestamp"].as_str().is_some());
        assert_eq!(payload["event"]["flow"]["id"], flow_id);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_source_updated_dispatches_on_tag_change() {
        use tokio::io::AsyncReadExt;

        let (service, _store, _tmp) = test_service().await;

        // Create a flow first (auto-creates source)
        let source_id = uuid::Uuid::new_v4().to_string();
        let flow_id = uuid::Uuid::new_v4().to_string();
        let resp = TestClient::put(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "id": &flow_id,
                "source_id": &source_id,
                "format": "urn:x-nmos:format:video",
                "codec": "video/h264",
                "essence_parameters": {
                    "frame_rate": {"numerator": 25, "denominator": 1},
                    "frame_width": 1920,
                    "frame_height": 1080
                }
            }))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201, "Flow PUT must succeed");

        // Bind listener
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let webhook_url = format!("http://127.0.0.1:{}/hook", addr.port());

        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).await.unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
            tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes())
                .await
                .unwrap();
            let _ = tx.send(request);
        });

        // Register webhook for sources/updated
        TestClient::post("http://localhost:5800/service/webhooks")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "url": webhook_url,
                "events": ["sources/updated"]
            }))
            .send(&service)
            .await;

        // Update a source tag — should trigger sources/updated
        TestClient::put(format!(
            "http://localhost:5800/sources/{source_id}/tags/genre"
        ))
        .basic_auth("test", Some("password"))
        .json(&serde_json::json!("news"))
        .send(&service)
        .await;

        // Wait for the webhook POST
        let request = tokio::time::timeout(std::time::Duration::from_secs(5), rx)
            .await
            .expect("Timed out waiting for webhook POST")
            .expect("Webhook receiver task failed");

        let body_start = request.find("\r\n\r\n").expect("No body separator") + 4;
        let payload: serde_json::Value =
            serde_json::from_str(&request[body_start..]).expect("Invalid JSON");

        assert_eq!(payload["event_type"], "sources/updated");
        assert_eq!(payload["event"]["source"]["id"], source_id);
    }
}

use salvo::http::StatusCode;
use salvo::prelude::*;

use crate::error::AppError;
use crate::handlers::{flow_id, get_store, parse_json, tag_name};
use tams_types::tags::TagValue;

// -- Tags --

/// GET /flows/{flowId}/tags
#[handler]
pub async fn get_flow_tags(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    match store.get_flow_tags(&id).await {
        Some(tags) => res.render(Json(tags)),
        None => AppError::not_found(format!("Flow {id} not found")).write_to(res),
    }
}

/// GET /flows/{flowId}/tags/{name}
#[handler]
pub async fn get_flow_tag(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    let name = tag_name(req);
    match store.get_flow_tag(&id, &name).await {
        Some(value) => res.render(Json(value)),
        None => AppError::not_found(format!("Tag {name} not found on flow {id}")).write_to(res),
    }
}

/// PUT /flows/{flowId}/tags/{name}
#[handler]
pub async fn put_flow_tag(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    let name = tag_name(req);
    let Some(value) = parse_json::<TagValue>(req, res).await else {
        return;
    };
    match store.set_flow_tag(&id, &name, value).await {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

/// DELETE /flows/{flowId}/tags/{name}
#[handler]
pub async fn delete_flow_tag(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    let name = tag_name(req);
    match store.delete_flow_tag(&id, &name).await {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

// -- Label --

/// GET /flows/{flowId}/label
#[handler]
pub async fn get_flow_label(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    match store.get_flow_property(&id, "label").await {
        Some(Some(val)) => res.render(Json(val)),
        Some(None) | None => {
            AppError::not_found(format!("Flow {id} not found or label not set")).write_to(res)
        }
    }
}

/// PUT /flows/{flowId}/label
#[handler]
pub async fn put_flow_label(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    let Some(value) = parse_json::<String>(req, res).await else {
        return;
    };
    match store
        .set_flow_property(&id, "label", serde_json::Value::String(value))
        .await
    {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

/// DELETE /flows/{flowId}/label
#[handler]
pub async fn delete_flow_label(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    match store.delete_flow_property(&id, "label").await {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

// -- Description --

/// GET /flows/{flowId}/description
#[handler]
pub async fn get_flow_description(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    match store.get_flow_property(&id, "description").await {
        Some(Some(val)) => res.render(Json(val)),
        Some(None) | None => {
            AppError::not_found(format!("Flow {id} not found or description not set")).write_to(res)
        }
    }
}

/// PUT /flows/{flowId}/description
#[handler]
pub async fn put_flow_description(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    let Some(value) = parse_json::<String>(req, res).await else {
        return;
    };
    match store
        .set_flow_property(&id, "description", serde_json::Value::String(value))
        .await
    {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

/// DELETE /flows/{flowId}/description
#[handler]
pub async fn delete_flow_description(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    match store.delete_flow_property(&id, "description").await {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

// -- read_only --

/// GET /flows/{flowId}/read_only
#[handler]
pub async fn get_flow_read_only(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    match store.get_flow_read_only(&id).await {
        Some(val) => res.render(Json(val)),
        None => AppError::not_found(format!("Flow {id} not found")).write_to(res),
    }
}

/// PUT /flows/{flowId}/read_only
#[handler]
pub async fn put_flow_read_only(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    let Some(value) = parse_json::<bool>(req, res).await else {
        return;
    };
    match store.set_flow_read_only(&id, value).await {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

// -- flow_collection --

/// GET /flows/{flowId}/flow_collection
#[handler]
pub async fn get_flow_collection(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    match store.get_flow_property(&id, "flow_collection").await {
        Some(Some(val)) => res.render(Json(val)),
        Some(None) | None => {
            AppError::not_found(format!("Flow {id} not found or flow_collection not set"))
                .write_to(res)
        }
    }
}

/// PUT /flows/{flowId}/flow_collection
#[handler]
pub async fn put_flow_collection(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    let Some(value) = parse_json::<serde_json::Value>(req, res).await else {
        return;
    };
    // Validate it's an array of objects with id and role
    if let Some(arr) = value.as_array() {
        for item in arr {
            let obj = match item.as_object() {
                Some(o) => o,
                None => {
                    AppError::bad_request("Each flow_collection item must be an object")
                        .write_to(res);
                    return;
                }
            };
            if obj.get("id").and_then(|v| v.as_str()).is_none() {
                AppError::bad_request("Each flow_collection item must have a string id")
                    .write_to(res);
                return;
            }
            if obj.get("role").and_then(|v| v.as_str()).is_none() {
                AppError::bad_request("Each flow_collection item must have a string role")
                    .write_to(res);
                return;
            }
        }
    } else {
        AppError::bad_request("flow_collection must be a JSON array").write_to(res);
        return;
    }
    match store.set_flow_collection(&id, value).await {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

/// DELETE /flows/{flowId}/flow_collection
#[handler]
pub async fn delete_flow_collection(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    match store.delete_flow_collection(&id).await {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

// -- max_bit_rate --

/// GET /flows/{flowId}/max_bit_rate
#[handler]
pub async fn get_flow_max_bit_rate(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    match store.get_flow_property(&id, "max_bit_rate").await {
        Some(Some(val)) => res.render(Json(val)),
        Some(None) | None => {
            AppError::not_found(format!("Flow {id} not found or max_bit_rate not set"))
                .write_to(res)
        }
    }
}

/// PUT /flows/{flowId}/max_bit_rate
#[handler]
pub async fn put_flow_max_bit_rate(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    let Some(value) = parse_json::<serde_json::Value>(req, res).await else {
        return;
    };
    match value.as_i64() {
        Some(n) if n >= 0 => {}
        _ => {
            AppError::bad_request("max_bit_rate must be a non-negative integer").write_to(res);
            return;
        }
    }
    match store.set_flow_property(&id, "max_bit_rate", value).await {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

/// DELETE /flows/{flowId}/max_bit_rate
#[handler]
pub async fn delete_flow_max_bit_rate(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    match store.delete_flow_property(&id, "max_bit_rate").await {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

// -- avg_bit_rate --

/// GET /flows/{flowId}/avg_bit_rate
#[handler]
pub async fn get_flow_avg_bit_rate(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    match store.get_flow_property(&id, "avg_bit_rate").await {
        Some(Some(val)) => res.render(Json(val)),
        Some(None) | None => {
            AppError::not_found(format!("Flow {id} not found or avg_bit_rate not set"))
                .write_to(res)
        }
    }
}

/// PUT /flows/{flowId}/avg_bit_rate
#[handler]
pub async fn put_flow_avg_bit_rate(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    let Some(value) = parse_json::<serde_json::Value>(req, res).await else {
        return;
    };
    match value.as_i64() {
        Some(n) if n >= 0 => {}
        _ => {
            AppError::bad_request("avg_bit_rate must be a non-negative integer").write_to(res);
            return;
        }
    }
    match store.set_flow_property(&id, "avg_bit_rate", value).await {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

/// DELETE /flows/{flowId}/avg_bit_rate
#[handler]
pub async fn delete_flow_avg_bit_rate(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    match store.delete_flow_property(&id, "avg_bit_rate").await {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

#[cfg(test)]
mod tests {
    use salvo::http::StatusCode;
    use salvo::prelude::*;
    use salvo::test::{ResponseExt, TestClient};

    use crate::test_utils::test_service;
    use tams_store::Store;

    fn auth() -> (&'static str, Option<&'static str>) {
        ("test", Some("password"))
    }

    fn video_flow_json(id: &str, source_id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "source_id": source_id,
            "format": "urn:x-nmos:format:video",
            "codec": "video/h264",
            "container": "video/mp2t",
            "tags": {"genre": "news"},
            "essence_parameters": {
                "frame_width": 1920,
                "frame_height": 1080
            }
        })
    }

    const FLOW_ID: &str = "f0000000-0000-0000-0000-000000000001";
    const SOURCE_ID: &str = "s0000000-0000-0000-0000-000000000001";

    async fn service_with_flow() -> (Service, Store, tempfile::TempDir) {
        let (svc, store, tmp) = test_service().await;
        store
            .put_flow(video_flow_json(FLOW_ID, SOURCE_ID))
            .await
            .unwrap();
        (svc, store, tmp)
    }

    // ========== TAGS ==========

    #[tokio::test]
    async fn get_flow_tags_returns_tags_object() {
        let (svc, _store, _tmp) = service_with_flow().await;
        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}/tags"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body, serde_json::json!({"genre": "news"}));
    }

    #[tokio::test]
    async fn get_flow_tags_not_found() {
        let (svc, _store, _tmp) = test_service().await;
        let resp = TestClient::get("http://localhost/flows/nonexistent/tags")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_flow_tag_single_value() {
        let (svc, _store, _tmp) = service_with_flow().await;
        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}/tags/genre"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body, serde_json::json!("news"));
    }

    #[tokio::test]
    async fn get_flow_tag_not_found() {
        let (svc, _store, _tmp) = service_with_flow().await;
        let resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}/tags/nonexistent"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn put_flow_tag_creates_and_get_returns_it() {
        let (svc, _store, _tmp) = service_with_flow().await;
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}/tags/quality"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!("high"))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}/tags/quality"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body, serde_json::json!("high"));
    }

    #[tokio::test]
    async fn delete_flow_tag_removes_it() {
        let (svc, _store, _tmp) = service_with_flow().await;
        let resp = TestClient::delete(format!("http://localhost/flows/{FLOW_ID}/tags/genre"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        let resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}/tags/genre"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn put_flow_tag_on_read_only_returns_403() {
        let (svc, store, _tmp) = service_with_flow().await;
        store.set_flow_read_only(FLOW_ID, true).await.unwrap();
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}/tags/quality"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!("high"))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn delete_flow_tag_on_read_only_returns_403() {
        let (svc, store, _tmp) = service_with_flow().await;
        store.set_flow_read_only(FLOW_ID, true).await.unwrap();
        let resp = TestClient::delete(format!("http://localhost/flows/{FLOW_ID}/tags/genre"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn put_flow_tag_updates_metadata() {
        let (svc, _store, _tmp) = service_with_flow().await;
        // Get original metadata_version
        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let original: serde_json::Value = resp.take_json().await.unwrap();
        let orig_version = original["metadata_version"].as_str().unwrap().to_string();

        // Modify a tag
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}/tags/new_tag"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!("value"))
            .send(&svc)
            .await;

        // Verify metadata_version changed
        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let updated: serde_json::Value = resp.take_json().await.unwrap();
        let new_version = updated["metadata_version"].as_str().unwrap();
        assert_ne!(new_version, orig_version);
    }

    // ========== LABEL ==========

    #[tokio::test]
    async fn get_flow_label_not_set_returns_404() {
        // Create a flow without a label
        let (svc, store, _tmp) = test_service().await;
        store
            .put_flow(serde_json::json!({
                "id": FLOW_ID,
                "source_id": SOURCE_ID,
                "format": "urn:x-nmos:format:data"
            }))
            .await
            .unwrap();
        let resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}/label"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn put_flow_label_then_get() {
        let (svc, _store, _tmp) = service_with_flow().await;
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}/label"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!("New Label"))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}/label"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body, serde_json::json!("New Label"));
    }

    #[tokio::test]
    async fn delete_flow_label_then_get_returns_404() {
        let (svc, _store, _tmp) = service_with_flow().await;
        // Flow was created with label "Test Flow" — delete it
        let resp = TestClient::delete(format!("http://localhost/flows/{FLOW_ID}/label"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        let resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}/label"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn put_flow_label_on_read_only_returns_403() {
        let (svc, store, _tmp) = service_with_flow().await;
        store.set_flow_read_only(FLOW_ID, true).await.unwrap();
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}/label"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!("New Label"))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::FORBIDDEN);
    }

    // ========== DESCRIPTION ==========

    #[tokio::test]
    async fn put_flow_description_then_get() {
        let (svc, _store, _tmp) = service_with_flow().await;
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}/description"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!("Updated description"))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}/description"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body, serde_json::json!("Updated description"));
    }

    #[tokio::test]
    async fn delete_flow_description_then_get_returns_404() {
        let (svc, _store, _tmp) = service_with_flow().await;
        let resp = TestClient::delete(format!("http://localhost/flows/{FLOW_ID}/description"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        let resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}/description"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    // ========== READ_ONLY ==========

    #[tokio::test]
    async fn get_flow_read_only_defaults_false() {
        let (svc, _store, _tmp) = service_with_flow().await;
        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}/read_only"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body, serde_json::json!(false));
    }

    #[tokio::test]
    async fn put_flow_read_only_true_then_get() {
        let (svc, _store, _tmp) = service_with_flow().await;
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}/read_only"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!(true))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}/read_only"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body, serde_json::json!(true));
    }

    #[tokio::test]
    async fn put_read_only_back_to_false_when_read_only() {
        let (svc, store, _tmp) = service_with_flow().await;
        store.set_flow_read_only(FLOW_ID, true).await.unwrap();

        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}/read_only"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!(false))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}/read_only"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body, serde_json::json!(false));
    }

    #[tokio::test]
    async fn put_read_only_true_when_already_read_only_succeeds() {
        let (svc, store, _tmp) = service_with_flow().await;
        store.set_flow_read_only(FLOW_ID, true).await.unwrap();

        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}/read_only"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!(true))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn read_only_flow_rejects_description_put() {
        let (svc, store, _tmp) = service_with_flow().await;
        store.set_flow_read_only(FLOW_ID, true).await.unwrap();

        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}/description"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!("nope"))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn read_only_flow_rejects_description_delete() {
        let (svc, store, _tmp) = service_with_flow().await;
        store.set_flow_read_only(FLOW_ID, true).await.unwrap();

        let resp = TestClient::delete(format!("http://localhost/flows/{FLOW_ID}/description"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::FORBIDDEN);
    }

    // ========== FLOW_COLLECTION ==========

    #[tokio::test]
    async fn put_flow_collection_then_get() {
        let (svc, store, _tmp) = service_with_flow().await;
        // Create a child flow first (collection items must exist)
        let child_id = "f0000000-0000-0000-0000-000000000002";
        store
            .put_flow(serde_json::json!({
                "id": child_id,
                "source_id": SOURCE_ID,
                "format": "urn:x-nmos:format:data"
            }))
            .await
            .unwrap();

        let collection = serde_json::json!([
            {"id": child_id, "role": "video"}
        ]);
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}/flow_collection"))
            .basic_auth(auth().0, auth().1)
            .json(&collection)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}/flow_collection"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body, collection);
    }

    #[tokio::test]
    async fn put_flow_collection_updates_collected_by() {
        let (svc, store, _tmp) = service_with_flow().await;
        let child_id = "f0000000-0000-0000-0000-000000000002";
        store
            .put_flow(serde_json::json!({
                "id": child_id,
                "source_id": SOURCE_ID,
                "format": "urn:x-nmos:format:data"
            }))
            .await
            .unwrap();

        let collection = serde_json::json!([
            {"id": child_id, "role": "video"}
        ]);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}/flow_collection"))
            .basic_auth(auth().0, auth().1)
            .json(&collection)
            .send(&svc)
            .await;

        // Check child flow's collected_by includes parent
        let mut resp = TestClient::get(format!("http://localhost/flows/{child_id}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let child: serde_json::Value = resp.take_json().await.unwrap();
        let collected_by = child["collected_by"].as_array().unwrap();
        assert!(collected_by.contains(&serde_json::json!(FLOW_ID)));
    }

    #[tokio::test]
    async fn put_flow_collection_nonexistent_child_returns_400() {
        let (svc, _store, _tmp) = service_with_flow().await;
        let collection = serde_json::json!([
            {"id": "f9999999-9999-9999-9999-999999999999", "role": "video"}
        ]);
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}/flow_collection"))
            .basic_auth(auth().0, auth().1)
            .json(&collection)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn delete_flow_collection_clears_collected_by() {
        let (svc, store, _tmp) = service_with_flow().await;
        let child_id = "f0000000-0000-0000-0000-000000000002";
        store
            .put_flow(serde_json::json!({
                "id": child_id,
                "source_id": SOURCE_ID,
                "format": "urn:x-nmos:format:data"
            }))
            .await
            .unwrap();

        // Set collection
        let collection = serde_json::json!([{"id": child_id, "role": "video"}]);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}/flow_collection"))
            .basic_auth(auth().0, auth().1)
            .json(&collection)
            .send(&svc)
            .await;

        // Delete collection
        let resp = TestClient::delete(format!("http://localhost/flows/{FLOW_ID}/flow_collection"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        // Child's collected_by should no longer include parent
        let mut resp = TestClient::get(format!("http://localhost/flows/{child_id}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let child: serde_json::Value = resp.take_json().await.unwrap();
        let collected_by = child
            .get("collected_by")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        assert!(!collected_by.contains(&serde_json::json!(FLOW_ID)));
    }

    #[tokio::test]
    async fn put_flow_collection_on_read_only_returns_403() {
        let (svc, store, _tmp) = service_with_flow().await;
        store.set_flow_read_only(FLOW_ID, true).await.unwrap();
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}/flow_collection"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!([]))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::FORBIDDEN);
    }

    // ========== BIT RATES ==========

    #[tokio::test]
    async fn put_max_bit_rate_then_get() {
        let (svc, _store, _tmp) = service_with_flow().await;
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}/max_bit_rate"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!(5000))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}/max_bit_rate"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body, serde_json::json!(5000));
    }

    #[tokio::test]
    async fn delete_max_bit_rate_then_get_returns_404() {
        let (svc, _store, _tmp) = service_with_flow().await;
        // Set it first
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}/max_bit_rate"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!(5000))
            .send(&svc)
            .await;

        let resp = TestClient::delete(format!("http://localhost/flows/{FLOW_ID}/max_bit_rate"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        let resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}/max_bit_rate"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn put_avg_bit_rate_then_get() {
        let (svc, _store, _tmp) = service_with_flow().await;
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}/avg_bit_rate"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!(3246))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}/avg_bit_rate"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body, serde_json::json!(3246));
    }

    #[tokio::test]
    async fn delete_avg_bit_rate_then_get_returns_404() {
        let (svc, _store, _tmp) = service_with_flow().await;
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}/avg_bit_rate"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!(3000))
            .send(&svc)
            .await;

        let resp = TestClient::delete(format!("http://localhost/flows/{FLOW_ID}/avg_bit_rate"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        let resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}/avg_bit_rate"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn put_negative_bit_rate_returns_400() {
        let (svc, _store, _tmp) = service_with_flow().await;
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}/max_bit_rate"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!(-1))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn put_bit_rate_on_read_only_returns_403() {
        let (svc, store, _tmp) = service_with_flow().await;
        store.set_flow_read_only(FLOW_ID, true).await.unwrap();
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}/max_bit_rate"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!(5000))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn delete_label_on_read_only_returns_403() {
        let (svc, store, _tmp) = service_with_flow().await;
        store
            .set_flow_property(FLOW_ID, "label", serde_json::json!("test"))
            .await
            .unwrap();
        store.set_flow_read_only(FLOW_ID, true).await.unwrap();
        let resp = TestClient::delete(format!("http://localhost/flows/{FLOW_ID}/label"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn delete_flow_collection_on_read_only_returns_403() {
        let (svc, store, _tmp) = service_with_flow().await;
        store.set_flow_read_only(FLOW_ID, true).await.unwrap();
        let resp = TestClient::delete(format!("http://localhost/flows/{FLOW_ID}/flow_collection"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn delete_max_bit_rate_on_read_only_returns_403() {
        let (svc, store, _tmp) = service_with_flow().await;
        store
            .set_flow_property(FLOW_ID, "max_bit_rate", serde_json::json!(5000))
            .await
            .unwrap();
        store.set_flow_read_only(FLOW_ID, true).await.unwrap();
        let resp = TestClient::delete(format!("http://localhost/flows/{FLOW_ID}/max_bit_rate"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn delete_avg_bit_rate_on_read_only_returns_403() {
        let (svc, store, _tmp) = service_with_flow().await;
        store
            .set_flow_property(FLOW_ID, "avg_bit_rate", serde_json::json!(3000))
            .await
            .unwrap();
        store.set_flow_read_only(FLOW_ID, true).await.unwrap();
        let resp = TestClient::delete(format!("http://localhost/flows/{FLOW_ID}/avg_bit_rate"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn put_avg_bit_rate_on_read_only_returns_403() {
        let (svc, store, _tmp) = service_with_flow().await;
        store.set_flow_read_only(FLOW_ID, true).await.unwrap();
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}/avg_bit_rate"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!(3000))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::FORBIDDEN);
    }

    // ========== HEAD ==========

    #[tokio::test]
    async fn head_flow_tags_returns_200_no_body() {
        let (svc, _store, _tmp) = service_with_flow().await;
        let resp = TestClient::head(format!("http://localhost/flows/{FLOW_ID}/tags"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
    }

    #[tokio::test]
    async fn head_flow_label_returns_200_when_set() {
        let (svc, store, _tmp) = service_with_flow().await;
        // Set a label first
        store
            .set_flow_property(
                FLOW_ID,
                "label",
                serde_json::Value::String("My Label".into()),
            )
            .await
            .unwrap();
        let resp = TestClient::head(format!("http://localhost/flows/{FLOW_ID}/label"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
    }

    #[tokio::test]
    async fn head_flow_read_only_returns_200() {
        let (svc, _store, _tmp) = service_with_flow().await;
        let resp = TestClient::head(format!("http://localhost/flows/{FLOW_ID}/read_only"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
    }

    // ========== FLOW NOT FOUND ==========

    #[tokio::test]
    async fn property_endpoints_return_404_for_missing_flow() {
        let (svc, _store, _tmp) = test_service().await;
        let bad = "f9999999-9999-9999-9999-999999999999";

        let resp = TestClient::get(format!("http://localhost/flows/{bad}/tags"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);

        let resp = TestClient::get(format!("http://localhost/flows/{bad}/label"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);

        let resp = TestClient::get(format!("http://localhost/flows/{bad}/read_only"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);

        let resp = TestClient::put(format!("http://localhost/flows/{bad}/label"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!("test"))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    // ========== LABEL GET RETURNS VALUE FROM FLOW CREATION ==========

    #[tokio::test]
    async fn get_flow_label_returns_404_when_not_set() {
        let (svc, _store, _tmp) = service_with_flow().await;
        // video_flow_json has no label field, so GET should return 404
        let resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}/label"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }
}

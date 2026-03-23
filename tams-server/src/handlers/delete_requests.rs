use salvo::prelude::*;

use crate::handlers::get_store;

/// GET /flow-delete-requests
#[handler]
pub async fn get_delete_requests(depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let requests = store.list_deletion_requests().await;
    res.render(Json(requests));
}

/// GET /flow-delete-requests/{request-id}
#[handler]
pub async fn get_delete_request(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = req
        .param::<String>("request-id")
        .expect("request-id path param missing from route");

    match store.get_deletion_request(&id).await {
        Some(request) => res.render(Json(request)),
        None => {
            crate::error::AppError::not_found(format!("Deletion request {id} not found"))
                .write_to(res);
        }
    }
}

#[cfg(test)]
mod tests {
    use salvo::http::StatusCode;
    use salvo::prelude::*;
    use salvo::test::{ResponseExt, TestClient};

    use crate::test_utils::test_service;

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
            "essence_parameters": {
                "frame_rate": {"numerator": 25, "denominator": 1},
                "frame_width": 1920,
                "frame_height": 1080
            }
        })
    }

    /// Create a flow and add segments so DELETE triggers async path.
    async fn create_flow_with_segments(service: &Service) -> String {
        let flow_id = uuid::Uuid::new_v4().to_string();
        let source_id = uuid::Uuid::new_v4().to_string();

        // Create flow
        let resp = TestClient::put(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth(auth().0, auth().1)
            .json(&video_flow_json(&flow_id, &source_id))
            .send(service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);

        // Allocate storage
        let mut resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/storage"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!({"limit": 1}))
            .send(service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);
        let storage: serde_json::Value = resp.take_json().await.unwrap();
        let object_id = storage["media_objects"][0]["object_id"]
            .as_str()
            .unwrap()
            .to_string();

        // Upload media
        TestClient::put(format!("http://localhost:5800/media/{object_id}"))
            .basic_auth(auth().0, auth().1)
            .bytes(b"fake-media-content".to_vec())
            .send(service)
            .await;

        // Post a segment
        let resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!([{
                "object_id": object_id,
                "timerange": "[0:0_5:0)"
            }]))
            .send(service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);

        flow_id
    }

    // -- GET /flow-delete-requests --

    #[tokio::test]
    async fn list_delete_requests_empty() {
        let (service, _store, _tmp) = test_service().await;
        let mut resp = TestClient::get("http://localhost:5800/flow-delete-requests")
            .basic_auth(auth().0, auth().1)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn head_delete_requests_no_body() {
        let (service, _store, _tmp) = test_service().await;
        let mut resp = TestClient::head("http://localhost:5800/flow-delete-requests")
            .basic_auth(auth().0, auth().1)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body = resp.take_string().await.unwrap();
        assert!(body.is_empty());
    }

    // -- DELETE /flows/{flowId} without segments → 204 --

    #[tokio::test]
    async fn delete_flow_no_segments_returns_204() {
        let (service, _store, _tmp) = test_service().await;
        let flow_id = uuid::Uuid::new_v4().to_string();
        let source_id = uuid::Uuid::new_v4().to_string();

        TestClient::put(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth(auth().0, auth().1)
            .json(&video_flow_json(&flow_id, &source_id))
            .send(&service)
            .await;

        let resp = TestClient::delete(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth(auth().0, auth().1)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);
        assert!(resp.headers().get("location").is_none());
    }

    // -- DELETE /flows/{flowId} with segments → 202 + Location --

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn delete_flow_with_segments_returns_202() {
        let (service, _store, _tmp) = test_service().await;
        let flow_id = create_flow_with_segments(&service).await;

        let mut resp = TestClient::delete(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth(auth().0, auth().1)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::ACCEPTED);

        // Check Location header
        let location = resp
            .headers()
            .get("location")
            .expect("202 must have Location header")
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            location.starts_with("/flow-delete-requests/"),
            "Location should point to deletion request, got: {location}"
        );

        // Check response body is a valid DeletionRequest
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["flow_id"], flow_id);
        assert_eq!(body["delete_flow"], true);
        assert_eq!(body["status"], "created");
        assert_eq!(body["timerange_to_delete"], "_");
    }

    // -- Deletion request lifecycle: created → started → done --

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn deletion_request_reaches_done() {
        let (service, _store, _tmp) = test_service().await;
        let flow_id = create_flow_with_segments(&service).await;

        // Delete the flow (async)
        let mut resp = TestClient::delete(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth(auth().0, auth().1)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::ACCEPTED);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let request_id = body["id"].as_str().unwrap().to_string();

        // Wait for the background task to complete
        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let mut resp = TestClient::get(format!(
                "http://localhost:5800/flow-delete-requests/{request_id}"
            ))
            .basic_auth(auth().0, auth().1)
            .send(&service)
            .await;
            let status_body: serde_json::Value = resp.take_json().await.unwrap();
            if status_body["status"] == "done" {
                // Verify fields
                assert_eq!(status_body["flow_id"], flow_id);
                assert!(status_body.get("updated").is_some());
                assert!(
                    status_body.get("timerange_remaining").is_none()
                        || status_body["timerange_remaining"].is_null()
                );
                // Verify the flow is actually gone
                let resp = TestClient::get(format!("http://localhost:5800/flows/{flow_id}"))
                    .basic_auth(auth().0, auth().1)
                    .send(&service)
                    .await;
                assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
                return;
            }
        }
        panic!("Deletion request did not reach 'done' status within 2.5 seconds");
    }

    // -- GET /flow-delete-requests lists the request --

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_contains_active_request() {
        let (service, _store, _tmp) = test_service().await;
        let flow_id = create_flow_with_segments(&service).await;

        TestClient::delete(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth(auth().0, auth().1)
            .send(&service)
            .await;

        let mut resp = TestClient::get("http://localhost:5800/flow-delete-requests")
            .basic_auth(auth().0, auth().1)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert!(
            !body.as_array().unwrap().is_empty(),
            "Should list at least one deletion request"
        );
    }

    // -- GET /flow-delete-requests/{id} for nonexistent → 404 --

    #[tokio::test]
    async fn get_delete_request_not_found() {
        let (service, _store, _tmp) = test_service().await;
        let resp = TestClient::get("http://localhost:5800/flow-delete-requests/nonexistent")
            .basic_auth(auth().0, auth().1)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    // -- HEAD /flow-delete-requests/{id} --

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn head_delete_request_no_body() {
        let (service, _store, _tmp) = test_service().await;
        let flow_id = create_flow_with_segments(&service).await;

        let mut resp = TestClient::delete(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth(auth().0, auth().1)
            .send(&service)
            .await;
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let request_id = body["id"].as_str().unwrap();

        let mut resp = TestClient::head(format!(
            "http://localhost:5800/flow-delete-requests/{request_id}"
        ))
        .basic_auth(auth().0, auth().1)
        .send(&service)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body = resp.take_string().await.unwrap();
        assert!(body.is_empty());
    }

    // -- Deletion request has correct schema fields --

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn deletion_request_schema_fields() {
        let (service, _store, _tmp) = test_service().await;
        let flow_id = create_flow_with_segments(&service).await;

        let mut resp = TestClient::delete(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth(auth().0, auth().1)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::ACCEPTED);
        let body: serde_json::Value = resp.take_json().await.unwrap();

        // Required fields per schemas/deletion-request.json
        assert!(body.get("id").is_some(), "must have id");
        assert!(body.get("flow_id").is_some(), "must have flow_id");
        assert!(
            body.get("timerange_to_delete").is_some(),
            "must have timerange_to_delete"
        );
        assert!(body.get("delete_flow").is_some(), "must have delete_flow");
        assert!(body.get("status").is_some(), "must have status");
        // Optional fields
        assert!(body.get("created").is_some(), "should have created");
        assert!(body.get("created_by").is_some(), "should have created_by");
    }

    // -- DELETE on read_only flow returns 403 --

    #[tokio::test]
    async fn delete_readonly_flow_returns_403() {
        let (service, _store, _tmp) = test_service().await;
        let flow_id = uuid::Uuid::new_v4().to_string();
        let source_id = uuid::Uuid::new_v4().to_string();

        // Create flow
        TestClient::put(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth(auth().0, auth().1)
            .json(&video_flow_json(&flow_id, &source_id))
            .send(&service)
            .await;

        // Mark read_only
        TestClient::put(format!("http://localhost:5800/flows/{flow_id}/read_only"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!(true))
            .send(&service)
            .await;

        // Try to delete
        let resp = TestClient::delete(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth(auth().0, auth().1)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::FORBIDDEN);
    }
}

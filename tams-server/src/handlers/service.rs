use salvo::prelude::*;

use crate::error::AppError;
use crate::handlers::{get_store, parse_json};
use tams_types::service::ServicePost;

/// GET / -- list root API endpoints.
#[handler]
pub async fn get_root(res: &mut Response) {
    res.render(Json(serde_json::json!([
        "service",
        "flows",
        "sources",
        "flow-delete-requests"
    ])));
}

/// GET /service -- return service information.
#[handler]
pub async fn get_service(depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let info = store.get_service_info().await;
    res.render(Json(info));
}

/// POST /service -- update mutable service fields (name, description).
#[handler]
pub async fn post_service(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(post) = parse_json::<ServicePost>(req, res).await else {
        return;
    };
    let store = get_store(depot);
    match store.update_service_info(post).await {
        Ok(info) => res.render(Json(info)),
        Err(e) => {
            AppError::bad_request(format!("Failed to update service: {e}")).write_to(res);
        }
    }
}

/// GET /service/storage-backends -- list storage backends.
#[handler]
pub async fn get_storage_backends(depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    res.render(Json(store.storage_backends()));
}

#[cfg(test)]
mod tests {
    use salvo::http::StatusCode;
    use salvo::test::{ResponseExt, TestClient};

    use crate::test_utils::test_service;

    // -- GET / --

    #[tokio::test]
    async fn get_root_returns_endpoints() {
        let (svc, _store, _tmp) = test_service().await;
        let mut resp = TestClient::get("http://localhost/")
            .basic_auth("test", Some("password"))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<String> = resp.take_json().await.unwrap();
        assert_eq!(
            body,
            vec!["service", "flows", "sources", "flow-delete-requests"]
        );
    }

    // -- GET /service --

    #[tokio::test]
    async fn get_service_returns_valid_info() {
        let (svc, _store, _tmp) = test_service().await;
        let mut resp = TestClient::get("http://localhost/service")
            .basic_auth("test", Some("password"))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["api_version"], "8.0");
        assert_eq!(body["type"], "urn:x-tams:service:rustytams");
        assert!(body["min_object_timeout"].is_string());
    }

    #[tokio::test]
    async fn get_service_has_required_fields() {
        let (svc, _store, _tmp) = test_service().await;
        let mut resp = TestClient::get("http://localhost/service")
            .basic_auth("test", Some("password"))
            .send(&svc)
            .await;
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert!(body.get("type").is_some());
        assert!(body.get("api_version").is_some());
        assert!(body.get("min_object_timeout").is_some());
    }

    // -- POST /service --

    #[tokio::test]
    async fn post_service_updates_name() {
        let (svc, _store, _tmp) = test_service().await;
        let resp = TestClient::post("http://localhost/service")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"name": "Updated Name"}))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);

        let mut resp = TestClient::get("http://localhost/service")
            .basic_auth("test", Some("password"))
            .send(&svc)
            .await;
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["name"], "Updated Name");
    }

    #[tokio::test]
    async fn post_service_updates_description() {
        let (svc, _store, _tmp) = test_service().await;
        let resp = TestClient::post("http://localhost/service")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"description": "New description"}))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);

        let mut resp = TestClient::get("http://localhost/service")
            .basic_auth("test", Some("password"))
            .send(&svc)
            .await;
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["description"], "New description");
    }

    #[tokio::test]
    async fn post_service_preserves_other_fields() {
        let (svc, _store, _tmp) = test_service().await;
        TestClient::post("http://localhost/service")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"name": "New Name"}))
            .send(&svc)
            .await;

        let mut resp = TestClient::get("http://localhost/service")
            .basic_auth("test", Some("password"))
            .send(&svc)
            .await;
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["name"], "New Name");
        assert_eq!(body["api_version"], "8.0");
        assert_eq!(body["type"], "urn:x-tams:service:rustytams");
    }

    // -- GET /service/storage-backends --

    #[tokio::test]
    async fn get_storage_backends_returns_at_least_one() {
        let (svc, _store, _tmp) = test_service().await;
        let mut resp = TestClient::get("http://localhost/service/storage-backends")
            .basic_auth("test", Some("password"))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(!body.is_empty());
    }

    #[tokio::test]
    async fn get_storage_backends_has_required_fields() {
        let (svc, _store, _tmp) = test_service().await;
        let mut resp = TestClient::get("http://localhost/service/storage-backends")
            .basic_auth("test", Some("password"))
            .send(&svc)
            .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        let backend = &body[0];
        assert!(backend["id"].is_string());
        assert!(backend["store_type"].is_string());
        assert!(backend["provider"].is_string());
        assert!(backend["store_product"].is_string());
        assert_eq!(backend["default_storage"], true);
    }

    // -- HEAD variants --

    #[tokio::test]
    async fn head_root_returns_ok_no_body() {
        let (svc, _store, _tmp) = test_service().await;
        let mut resp = TestClient::head("http://localhost/")
            .basic_auth("test", Some("password"))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body = resp.take_string().await.unwrap();
        assert!(body.is_empty(), "HEAD should have no body, got: {body}");
    }

    #[tokio::test]
    async fn head_service_returns_ok_no_body() {
        let (svc, _store, _tmp) = test_service().await;
        let mut resp = TestClient::head("http://localhost/service")
            .basic_auth("test", Some("password"))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body = resp.take_string().await.unwrap();
        assert!(body.is_empty(), "HEAD should have no body, got: {body}");
    }

    #[tokio::test]
    async fn head_storage_backends_returns_ok_no_body() {
        let (svc, _store, _tmp) = test_service().await;
        let mut resp = TestClient::head("http://localhost/service/storage-backends")
            .basic_auth("test", Some("password"))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body = resp.take_string().await.unwrap();
        assert!(body.is_empty(), "HEAD should have no body, got: {body}");
    }

    // -- 404 --

    #[tokio::test]
    async fn nonexistent_path_returns_404_json() {
        let (svc, _store, _tmp) = test_service().await;
        let mut resp = TestClient::get("http://localhost/nonexistent")
            .basic_auth("test", Some("password"))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert!(body["type"].is_string());
        assert!(body["summary"].is_string());
        assert!(body["time"].is_string());
    }
}

use salvo::http::StatusCode;
use salvo::prelude::*;

use crate::error::AppError;
use crate::handlers::{flow_id, get_store, parse_json};
use tams_types::segment::StorageRequest;

/// POST /flows/{flowId}/storage
#[handler]
pub async fn post_storage(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let fid = flow_id(req);

    let body: serde_json::Value = match parse_json(req, res).await {
        Some(v) => v,
        None => return,
    };

    let request = StorageRequest {
        limit: body.get("limit").and_then(|v| v.as_u64()),
        object_ids: body.get("object_ids").and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
        }),
        storage_id: body
            .get("storage_id")
            .and_then(|v| v.as_str())
            .map(String::from),
    };

    match store.allocate_storage(&fid, request).await {
        Ok(objects) => {
            let media_objects: Vec<serde_json::Value> = objects
                .into_iter()
                .map(|obj| {
                    let mut put_url = serde_json::json!({
                        "url": obj.put_url,
                    });
                    if let Some(ct) = obj.content_type {
                        put_url
                            .as_object_mut()
                            .unwrap()
                            .insert("content-type".into(), serde_json::Value::String(ct));
                    }
                    serde_json::json!({
                        "object_id": obj.object_id,
                        "put_url": put_url
                    })
                })
                .collect();
            res.status_code(StatusCode::CREATED);
            res.render(Json(serde_json::json!({
                "media_objects": media_objects
            })));
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

#[cfg(test)]
mod tests {
    use salvo::test::{ResponseExt, TestClient};

    use crate::test_utils::test_service;

    fn video_flow_with_container(flow_id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": flow_id,
            "source_id": "src-storage",
            "format": "urn:x-nmos:format:video",
            "codec": "video/h264",
            "container": "video/mp2t",
            "essence_parameters": {
                "frame_width": 1920,
                "frame_height": 1080
            }
        })
    }

    fn video_flow_no_container(flow_id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": flow_id,
            "source_id": "src-storage",
            "format": "urn:x-nmos:format:video",
            "codec": "video/h264",
            "essence_parameters": {
                "frame_width": 1920,
                "frame_height": 1080
            }
        })
    }

    // -- POST /flows/{flowId}/storage --

    #[tokio::test]
    async fn storage_post_returns_media_objects() {
        let (service, _store, _tmp) = test_service().await;
        let flow_id = "f-stor-1";
        TestClient::put(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth("test", Some("password"))
            .json(&video_flow_with_container(flow_id))
            .send(&service)
            .await;

        let mut resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/storage"))
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"limit": 3}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let objects = body["media_objects"].as_array().unwrap();
        assert_eq!(objects.len(), 3);
        for obj in objects {
            assert!(obj["object_id"].is_string());
            assert!(obj["put_url"]["url"]
                .as_str()
                .unwrap()
                .contains(&format!("/{}/", tams_store::TEST_S3_BUCKET)));
        }
    }

    #[tokio::test]
    async fn storage_post_with_object_ids() {
        let (service, _store, _tmp) = test_service().await;
        let flow_id = "f-stor-2";
        TestClient::put(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth("test", Some("password"))
            .json(&video_flow_with_container(flow_id))
            .send(&service)
            .await;

        let mut resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/storage"))
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"object_ids": ["my-obj-1", "my-obj-2"]}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let objects = body["media_objects"].as_array().unwrap();
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0]["object_id"], "my-obj-1");
        assert_eq!(objects[1]["object_id"], "my-obj-2");
    }

    #[tokio::test]
    async fn storage_post_rejects_both_limit_and_object_ids() {
        let (service, _store, _tmp) = test_service().await;
        let flow_id = "f-stor-3";
        TestClient::put(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth("test", Some("password"))
            .json(&video_flow_with_container(flow_id))
            .send(&service)
            .await;

        let resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/storage"))
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"limit": 3, "object_ids": ["id1"]}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 400);
    }

    #[tokio::test]
    async fn storage_post_no_container_returns_400() {
        let (service, _store, _tmp) = test_service().await;
        let flow_id = "f-stor-4";
        TestClient::put(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth("test", Some("password"))
            .json(&video_flow_no_container(flow_id))
            .send(&service)
            .await;

        let resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/storage"))
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"limit": 1}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 400);
    }

    #[tokio::test]
    async fn storage_post_readonly_returns_403() {
        let (service, _store, _tmp) = test_service().await;
        let flow_id = "f-stor-5";
        TestClient::put(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth("test", Some("password"))
            .json(&video_flow_with_container(flow_id))
            .send(&service)
            .await;
        TestClient::put(format!("http://localhost:5800/flows/{flow_id}/read_only"))
            .basic_auth("test", Some("password"))
            .json(&true)
            .send(&service)
            .await;

        let resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/storage"))
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"limit": 1}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 403);
    }

    #[tokio::test]
    async fn storage_post_nonexistent_flow_returns_404() {
        let (service, _store, _tmp) = test_service().await;
        let resp = TestClient::post("http://localhost:5800/flows/nonexistent/storage")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"limit": 1}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 404);
    }

    #[tokio::test]
    async fn storage_post_invalid_storage_id_returns_400() {
        let (service, _store, _tmp) = test_service().await;
        let flow_id = "f-stor-bad-sid";
        TestClient::put(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth("test", Some("password"))
            .json(&video_flow_with_container(flow_id))
            .send(&service)
            .await;

        let resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/storage"))
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "limit": 1,
                "storage_id": "00000000-0000-0000-0000-000000000000"
            }))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 400);
    }

    #[tokio::test]
    async fn storage_post_no_limit_no_object_ids_defaults_to_one() {
        let (service, _store, _tmp) = test_service().await;
        let flow_id = "f-stor-default";
        TestClient::put(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth("test", Some("password"))
            .json(&video_flow_with_container(flow_id))
            .send(&service)
            .await;

        let mut resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/storage"))
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let objects = body["media_objects"].as_array().unwrap();
        assert_eq!(objects.len(), 1);
    }

    #[tokio::test]
    async fn storage_post_includes_content_type_from_container() {
        let (service, _store, _tmp) = test_service().await;
        let flow_id = "f-stor-ct";
        TestClient::put(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth("test", Some("password"))
            .json(&video_flow_with_container(flow_id))
            .send(&service)
            .await;

        let mut resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/storage"))
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"limit": 1}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let objects = body["media_objects"].as_array().unwrap();
        // container is "video/mp2t", should appear as content-type
        assert_eq!(
            objects[0]["put_url"]["content-type"].as_str().unwrap(),
            "video/mp2t"
        );
    }
}

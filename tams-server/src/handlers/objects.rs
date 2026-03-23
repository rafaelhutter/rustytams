use salvo::http::StatusCode;
use salvo::prelude::*;

use crate::error::AppError;
use crate::extract::tag_filters_from_request_with_prefix;
use crate::extract::{paginate_and_set_headers, pagination_from_request};
use crate::handlers::get_store;
use tams_types::object::{InstanceRequest, InstanceSelector, ObjectQuery};

/// GET /objects/{objectId}
#[handler]
pub async fn get_object(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let object_id = req
        .param::<String>("objectId")
        .expect("objectId path param missing from route");

    let query = parse_object_query(req);

    match store.get_object(&object_id, &query).await {
        Ok(info) => {
            // Paginate referenced_by_flows using shared utility
            let pagination = pagination_from_request(req);
            let page = paginate_and_set_headers(&info.referenced_by_flows, &pagination, req, res);

            let mut response = serde_json::json!({
                "id": info.id,
                "referenced_by_flows": page,
                "timerange": info.timerange.to_string(),
                "get_urls": info.get_urls,
            });

            if let Some(ref ff) = info.first_referenced_by_flow {
                response
                    .as_object_mut()
                    .unwrap()
                    .insert("first_referenced_by_flow".into(), serde_json::json!(ff));
            }

            res.render(Json(response));
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

/// POST /objects/{objectId}/instances
#[handler]
pub async fn post_object_instance(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let object_id = req
        .param::<String>("objectId")
        .expect("objectId path param missing from route");

    let body: serde_json::Value = match crate::handlers::parse_json(req, res).await {
        Some(v) => v,
        None => return,
    };

    let request = if let Some(storage_id) = body.get("storage_id").and_then(|v| v.as_str()) {
        InstanceRequest::Controlled {
            storage_id: storage_id.to_string(),
        }
    } else if let (Some(url), Some(label)) = (
        body.get("url").and_then(|v| v.as_str()),
        body.get("label").and_then(|v| v.as_str()),
    ) {
        InstanceRequest::Uncontrolled {
            url: url.to_string(),
            label: label.to_string(),
        }
    } else {
        crate::error::AppError::bad_request(
            "Must provide either 'storage_id' or both 'url' and 'label'",
        )
        .write_to(res);
        return;
    };

    match store.add_object_instance(&object_id, request).await {
        Ok(()) => {
            res.status_code(StatusCode::CREATED);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

/// DELETE /objects/{objectId}/instances
#[handler]
pub async fn delete_object_instance(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let object_id = req
        .param::<String>("objectId")
        .expect("objectId path param missing from route");

    let storage_id = req.query::<String>("storage_id");
    let label = req.query::<String>("label");

    let selector = if let Some(ref sid) = storage_id {
        InstanceSelector::ByStorageId(sid)
    } else if let Some(ref lbl) = label {
        InstanceSelector::ByLabel(lbl)
    } else {
        crate::error::AppError::bad_request("Must specify either storage_id or label")
            .write_to(res);
        return;
    };

    match store.delete_object_instance(&object_id, selector).await {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

/// Parse object query parameters from request.
fn parse_object_query(req: &Request) -> ObjectQuery {
    let presigned = req.query::<bool>("presigned");
    let accept_get_urls = req.query::<String>("accept_get_urls").map(|s| {
        if s.is_empty() {
            Vec::new()
        } else {
            s.split(',').map(|l| l.trim().to_string()).collect()
        }
    });
    let accept_storage_ids = req
        .query::<String>("accept_storage_ids")
        .filter(|s| !s.is_empty())
        .map(|s| s.split(',').map(|l| l.trim().to_string()).collect());
    let verbose_storage = req.query::<bool>("verbose_storage").unwrap_or(false);
    let flow_tag_filters = tag_filters_from_request_with_prefix(req, "flow_tag");

    ObjectQuery {
        presigned,
        accept_get_urls,
        accept_storage_ids,
        verbose_storage,
        flow_tag_filters,
    }
}

#[cfg(test)]
mod tests {
    use salvo::prelude::*;
    use salvo::test::{ResponseExt, TestClient};

    use crate::test_utils::test_service;

    fn video_flow(flow_id: &str, source_id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": flow_id,
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

    fn video_flow_with_tags(flow_id: &str, tags: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "id": flow_id,
            "source_id": "src-obj-tags",
            "format": "urn:x-nmos:format:video",
            "codec": "video/h264",
            "container": "video/mp2t",
            "tags": tags,
            "essence_parameters": {
                "frame_width": 1920,
                "frame_height": 1080
            }
        })
    }

    /// Helper: create a flow and post a segment referencing object_id.
    async fn create_flow_with_segment(
        service: &Service,
        flow_id: &str,
        source_id: &str,
        object_id: &str,
        timerange: &str,
    ) {
        TestClient::put(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth("test", Some("password"))
            .json(&video_flow(flow_id, source_id))
            .send(service)
            .await;
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "object_id": object_id,
                "timerange": timerange,
            }))
            .send(service)
            .await;
    }

    // -- GET /objects/{objectId} --

    #[tokio::test]
    async fn get_object_returns_metadata() {
        let (service, _store, _tmp) = test_service().await;
        create_flow_with_segment(&service, "f-obj-1", "src-obj-1", "obj-1", "[0:0_10:0)").await;

        let mut resp = TestClient::get("http://localhost:5800/objects/obj-1")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["id"], "obj-1");
        assert!(body["referenced_by_flows"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("f-obj-1")));
        assert_eq!(body["timerange"], "[0:0_10:0)");
        assert!(!body["get_urls"].as_array().unwrap().is_empty());
        let url = &body["get_urls"].as_array().unwrap().last().unwrap();
        assert!(url["url"]
            .as_str()
            .unwrap()
            .contains(&format!("/{}/obj-1", tams_store::TEST_S3_BUCKET)));
        // _paging must NOT leak to response
        assert!(body.get("_paging").is_none());
    }

    #[tokio::test]
    async fn get_object_nonexistent_returns_404() {
        let (service, _store, _tmp) = test_service().await;
        let resp = TestClient::get("http://localhost:5800/objects/nonexistent")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 404);
    }

    #[tokio::test]
    async fn get_object_allocated_but_no_segment_returns_404() {
        let (service, _store, _tmp) = test_service().await;
        TestClient::put("http://localhost:5800/flows/f-alloc")
            .basic_auth("test", Some("password"))
            .json(&video_flow("f-alloc", "src-alloc"))
            .send(&service)
            .await;
        TestClient::post("http://localhost:5800/flows/f-alloc/storage")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"object_ids": ["allocated-obj"]}))
            .send(&service)
            .await;
        TestClient::put("http://localhost:5800/media/allocated-obj")
            .basic_auth("test", Some("password"))
            .bytes(b"data".to_vec())
            .send(&service)
            .await;

        let resp = TestClient::get("http://localhost:5800/objects/allocated-obj")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 404);
    }

    #[tokio::test]
    async fn get_object_referenced_by_multiple_flows() {
        let (service, _store, _tmp) = test_service().await;
        create_flow_with_segment(
            &service,
            "f-multi-1",
            "src-multi",
            "shared-obj",
            "[0:0_5:0)",
        )
        .await;
        create_flow_with_segment(
            &service,
            "f-multi-2",
            "src-multi",
            "shared-obj",
            "[10:0_20:0)",
        )
        .await;

        let mut resp = TestClient::get("http://localhost:5800/objects/shared-obj")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let flows = body["referenced_by_flows"].as_array().unwrap();
        assert_eq!(flows.len(), 2);
        assert!(flows.contains(&serde_json::json!("f-multi-1")));
        assert!(flows.contains(&serde_json::json!("f-multi-2")));
        assert!(body["timerange"].as_str().unwrap().contains("0:0"));
        assert!(body["timerange"].as_str().unwrap().contains("20:0"));
    }

    #[tokio::test]
    async fn get_object_first_referenced_by_flow() {
        let (service, _store, _tmp) = test_service().await;
        create_flow_with_segment(&service, "f-first-1", "src-first", "first-obj", "[0:0_5:0)")
            .await;

        let mut resp = TestClient::get("http://localhost:5800/objects/first-obj")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["first_referenced_by_flow"], "f-first-1");
    }

    #[tokio::test]
    async fn get_object_flow_tag_filter() {
        let (service, _store, _tmp) = test_service().await;
        TestClient::put("http://localhost:5800/flows/f-tag-a")
            .basic_auth("test", Some("password"))
            .json(&video_flow_with_tags(
                "f-tag-a",
                serde_json::json!({"genre": "news"}),
            ))
            .send(&service)
            .await;
        TestClient::post("http://localhost:5800/flows/f-tag-a/segments")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"object_id": "tag-obj", "timerange": "[0:0_5:0)"}))
            .send(&service)
            .await;

        TestClient::put("http://localhost:5800/flows/f-tag-b")
            .basic_auth("test", Some("password"))
            .json(&video_flow_with_tags(
                "f-tag-b",
                serde_json::json!({"genre": "sport"}),
            ))
            .send(&service)
            .await;
        TestClient::post("http://localhost:5800/flows/f-tag-b/segments")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"object_id": "tag-obj", "timerange": "[10:0_20:0)"}))
            .send(&service)
            .await;

        let mut resp = TestClient::get("http://localhost:5800/objects/tag-obj?flow_tag.genre=news")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let flows = body["referenced_by_flows"].as_array().unwrap();
        assert_eq!(flows.len(), 1);
        assert_eq!(flows[0], "f-tag-a");
    }

    #[tokio::test]
    async fn get_object_flow_tag_exists_filter() {
        let (service, _store, _tmp) = test_service().await;
        TestClient::put("http://localhost:5800/flows/f-tagex")
            .basic_auth("test", Some("password"))
            .json(&video_flow_with_tags(
                "f-tagex",
                serde_json::json!({"special": "yes"}),
            ))
            .send(&service)
            .await;
        TestClient::post("http://localhost:5800/flows/f-tagex/segments")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"object_id": "tagex-obj", "timerange": "[0:0_5:0)"}))
            .send(&service)
            .await;

        TestClient::put("http://localhost:5800/flows/f-notag")
            .basic_auth("test", Some("password"))
            .json(&video_flow("f-notag", "src-notag"))
            .send(&service)
            .await;
        TestClient::post("http://localhost:5800/flows/f-notag/segments")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"object_id": "tagex-obj", "timerange": "[10:0_20:0)"}))
            .send(&service)
            .await;

        let mut resp =
            TestClient::get("http://localhost:5800/objects/tagex-obj?flow_tag_exists.special=true")
                .basic_auth("test", Some("password"))
                .send(&service)
                .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let flows = body["referenced_by_flows"].as_array().unwrap();
        assert_eq!(flows.len(), 1);
        assert_eq!(flows[0], "f-tagex");
    }

    #[tokio::test]
    async fn get_object_head_returns_no_body() {
        let (service, _store, _tmp) = test_service().await;
        create_flow_with_segment(&service, "f-head-obj", "src-head", "head-obj", "[0:0_5:0)").await;

        let mut resp = TestClient::head("http://localhost:5800/objects/head-obj")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body = resp.take_string().await.unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn get_object_verbose_storage() {
        let (service, _store, _tmp) = test_service().await;
        create_flow_with_segment(&service, "f-verbose", "src-v", "verbose-obj", "[0:0_5:0)").await;

        let mut resp =
            TestClient::get("http://localhost:5800/objects/verbose-obj?verbose_storage=true")
                .basic_auth("test", Some("password"))
                .send(&service)
                .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let urls = body["get_urls"].as_array().unwrap();
        assert!(urls.last().unwrap().get("storage_id").is_some());
        assert!(urls.last().unwrap().get("controlled").is_some());
    }

    #[tokio::test]
    async fn get_object_has_paging_headers() {
        let (service, _store, _tmp) = test_service().await;
        create_flow_with_segment(&service, "f-page", "src-page", "page-obj", "[0:0_5:0)").await;

        let resp = TestClient::get("http://localhost:5800/objects/page-obj")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        assert!(resp.headers().get("x-paging-limit").is_some());
    }

    // -- POST /objects/{objectId}/instances --

    #[tokio::test]
    async fn post_object_instance_uncontrolled() {
        let (service, _store, _tmp) = test_service().await;
        create_flow_with_segment(&service, "f-inst", "src-inst", "inst-obj", "[0:0_5:0)").await;

        let resp = TestClient::post("http://localhost:5800/objects/inst-obj/instances")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "url": "https://cdn.example.com/inst-obj",
                "label": "cdn-copy"
            }))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201);

        let mut resp = TestClient::get("http://localhost:5800/objects/inst-obj")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let urls = body["get_urls"].as_array().unwrap();
        assert!(urls.iter().any(|u| u["label"] == "cdn-copy"));
    }

    #[tokio::test]
    async fn post_object_instance_nonexistent_returns_404() {
        let (service, _store, _tmp) = test_service().await;
        let resp = TestClient::post("http://localhost:5800/objects/nonexistent/instances")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "url": "https://example.com/obj",
                "label": "test"
            }))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 404);
    }

    #[tokio::test]
    async fn post_object_instance_invalid_body_returns_400() {
        let (service, _store, _tmp) = test_service().await;
        create_flow_with_segment(
            &service,
            "f-bad-inst",
            "src-bad",
            "bad-inst-obj",
            "[0:0_5:0)",
        )
        .await;

        let resp = TestClient::post("http://localhost:5800/objects/bad-inst-obj/instances")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"foo": "bar"}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 400);
    }

    // -- DELETE /objects/{objectId}/instances --

    #[tokio::test]
    async fn delete_object_instance_by_label() {
        let (service, _store, _tmp) = test_service().await;
        create_flow_with_segment(
            &service,
            "f-del-inst",
            "src-del",
            "del-inst-obj",
            "[0:0_5:0)",
        )
        .await;

        TestClient::post("http://localhost:5800/objects/del-inst-obj/instances")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({
                "url": "https://cdn.example.com/del-inst-obj",
                "label": "cdn-to-delete"
            }))
            .send(&service)
            .await;

        let resp = TestClient::delete(
            "http://localhost:5800/objects/del-inst-obj/instances?label=cdn-to-delete",
        )
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        assert_eq!(resp.status_code.unwrap(), 204);

        let mut resp = TestClient::get("http://localhost:5800/objects/del-inst-obj")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let urls = body["get_urls"].as_array().unwrap();
        assert!(!urls.iter().any(|u| u["label"] == "cdn-to-delete"));
    }

    #[tokio::test]
    async fn delete_object_instance_nonexistent_returns_404() {
        let (service, _store, _tmp) = test_service().await;
        let resp =
            TestClient::delete("http://localhost:5800/objects/nonexistent/instances?label=test")
                .basic_auth("test", Some("password"))
                .send(&service)
                .await;
        assert_eq!(resp.status_code.unwrap(), 404);
    }

    #[tokio::test]
    async fn delete_object_instance_no_params_returns_400() {
        let (service, _store, _tmp) = test_service().await;
        create_flow_with_segment(
            &service,
            "f-del-noparam",
            "src-del-np",
            "del-np-obj",
            "[0:0_5:0)",
        )
        .await;

        let resp = TestClient::delete("http://localhost:5800/objects/del-np-obj/instances")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 400);
    }

    #[tokio::test]
    async fn delete_object_instance_nonexistent_label_returns_404() {
        let (service, _store, _tmp) = test_service().await;
        create_flow_with_segment(
            &service,
            "f-del-nolbl",
            "src-del-nl",
            "del-nl-obj",
            "[0:0_5:0)",
        )
        .await;

        let resp = TestClient::delete(
            "http://localhost:5800/objects/del-nl-obj/instances?label=nonexistent",
        )
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        assert_eq!(resp.status_code.unwrap(), 404);
    }

    #[tokio::test]
    async fn get_object_accept_get_urls_empty_removes_all() {
        let (service, _store, _tmp) = test_service().await;
        create_flow_with_segment(
            &service,
            "f-empty-urls",
            "src-eu",
            "empty-urls-obj",
            "[0:0_5:0)",
        )
        .await;

        let mut resp =
            TestClient::get("http://localhost:5800/objects/empty-urls-obj?accept_get_urls=")
                .basic_auth("test", Some("password"))
                .send(&service)
                .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let urls = body["get_urls"].as_array().unwrap();
        assert!(urls.is_empty());
    }

    // -- F3: POST controlled instance --

    /// Helper to discover the local storage backend ID from GET /service/storage-backends.
    async fn get_storage_backend_id(service: &Service) -> String {
        let mut resp = TestClient::get("http://localhost:5800/service/storage-backends")
            .basic_auth("test", Some("password"))
            .send(service)
            .await;
        let body: serde_json::Value = resp.take_json().await.unwrap();
        body.as_array().unwrap()[0]["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[tokio::test]
    async fn post_object_instance_controlled() {
        let (service, _store, _tmp) = test_service().await;
        create_flow_with_segment(
            &service,
            "f-ctrl-inst",
            "src-ctrl",
            "ctrl-inst-obj",
            "[0:0_5:0)",
        )
        .await;

        let storage_id = get_storage_backend_id(&service).await;
        let resp = TestClient::post("http://localhost:5800/objects/ctrl-inst-obj/instances")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"storage_id": storage_id}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201);
    }

    #[tokio::test]
    async fn post_object_instance_controlled_unknown_storage_returns_400() {
        let (service, _store, _tmp) = test_service().await;
        create_flow_with_segment(
            &service,
            "f-ctrl-bad",
            "src-ctrl-bad",
            "ctrl-bad-obj",
            "[0:0_5:0)",
        )
        .await;

        let resp = TestClient::post("http://localhost:5800/objects/ctrl-bad-obj/instances")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"storage_id": "nonexistent-storage"}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 400);
    }

    // -- M2: DELETE by storage_id --

    #[tokio::test]
    async fn delete_object_instance_by_storage_id_returns_400() {
        let (service, _store, _tmp) = test_service().await;
        create_flow_with_segment(
            &service,
            "f-del-sid",
            "src-del-sid",
            "del-sid-obj",
            "[0:0_5:0)",
        )
        .await;

        // Cannot delete the only controlled instance
        let storage_id = get_storage_backend_id(&service).await;
        let resp = TestClient::delete(format!(
            "http://localhost:5800/objects/del-sid-obj/instances?storage_id={storage_id}"
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        assert_eq!(resp.status_code.unwrap(), 400);
    }

    // -- M3: accept_get_urls filters by label --

    #[tokio::test]
    async fn get_object_accept_get_urls_filters_by_label() {
        let (service, _store, _tmp) = test_service().await;
        create_flow_with_segment(
            &service,
            "f-label-filt",
            "src-lf",
            "label-filt-obj",
            "[0:0_5:0)",
        )
        .await;

        // Add an uncontrolled instance with label "cdn"
        TestClient::post("http://localhost:5800/objects/label-filt-obj/instances")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"url": "https://cdn.example.com/obj", "label": "cdn"}))
            .send(&service)
            .await;

        // Filter accept_get_urls=cdn — should include only "cdn" label, not "local"
        let mut resp =
            TestClient::get("http://localhost:5800/objects/label-filt-obj?accept_get_urls=cdn")
                .basic_auth("test", Some("password"))
                .send(&service)
                .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let urls = body["get_urls"].as_array().unwrap();
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0]["label"], "cdn");
    }

    // -- M4: presigned=true excludes uncontrolled URLs --

    #[tokio::test]
    async fn get_object_presigned_excludes_uncontrolled() {
        let (service, _store, _tmp) = test_service().await;
        create_flow_with_segment(&service, "f-presign", "src-pre", "presign-obj", "[0:0_5:0)")
            .await;

        // Add an uncontrolled instance
        TestClient::post("http://localhost:5800/objects/presign-obj/instances")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"url": "https://cdn.example.com/p", "label": "cdn"}))
            .send(&service)
            .await;

        // presigned=true should include our presigned controlled URL but exclude uncontrolled
        let mut resp = TestClient::get("http://localhost:5800/objects/presign-obj?presigned=true")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let urls = body["get_urls"].as_array().unwrap();
        // Controlled URL is presigned (S3 presigned), so it should appear
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0]["label"], "s3");
    }

    // -- N3: verbose_storage=false omits controlled/storage_id --

    #[tokio::test]
    async fn get_object_non_verbose_omits_storage_fields() {
        let (service, _store, _tmp) = test_service().await;
        create_flow_with_segment(&service, "f-nonverb", "src-nv", "nonverb-obj", "[0:0_5:0)").await;

        // Add uncontrolled instance to test both URL types
        TestClient::post("http://localhost:5800/objects/nonverb-obj/instances")
            .basic_auth("test", Some("password"))
            .json(&serde_json::json!({"url": "https://cdn.example.com/nv", "label": "cdn"}))
            .send(&service)
            .await;

        let mut resp = TestClient::get("http://localhost:5800/objects/nonverb-obj")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let urls = body["get_urls"].as_array().unwrap();
        // Neither controlled nor uncontrolled URLs should have controlled/storage_id fields
        for url in urls {
            assert!(
                url.get("controlled").is_none(),
                "controlled field should be absent when verbose_storage=false: {url}"
            );
            assert!(
                url.get("storage_id").is_none(),
                "storage_id field should be absent when verbose_storage=false: {url}"
            );
        }
    }

    // -- M5: multi-page pagination for referenced_by_flows --

    #[tokio::test]
    async fn get_object_pagination_multi_page() {
        let (service, _store, _tmp) = test_service().await;
        // Create 3 flows referencing same object with non-overlapping timeranges
        let ranges = ["[0:0_10:0)", "[100:0_200:0)", "[300:0_400:0)"];
        for (i, tr) in ranges.iter().enumerate() {
            create_flow_with_segment(
                &service,
                &format!("f-pg-{i}"),
                &format!("src-pg-{i}"),
                "pg-obj",
                tr,
            )
            .await;
        }

        // Request with limit=2 to force pagination
        let mut resp = TestClient::get("http://localhost:5800/objects/pg-obj?limit=2")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let flows = body["referenced_by_flows"].as_array().unwrap();
        assert_eq!(flows.len(), 2);
        // Should have a Link next header
        let link = resp.headers().get("link");
        assert!(link.is_some(), "expected Link header for pagination");
        let link_str = link.unwrap().to_str().unwrap();
        assert!(link_str.contains("rel=\"next\""));
    }
}

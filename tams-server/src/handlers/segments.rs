use salvo::http::StatusCode;
use salvo::prelude::*;

use crate::error::AppError;
use crate::extract::{paginate_and_set_headers, pagination_from_request};
use crate::handlers::{flow_id, get_store, parse_json};
use tams_types::segment::{SegmentDeleteResult, SegmentPostResult, SegmentQuery};
use tams_types::timerange::TimeRange;

/// Parse and validate the timerange query parameter, writing a 400 error if invalid.
/// Returns `Ok(Some(tr))` for valid, `Ok(None)` for absent, `Err(())` if 400 was written.
fn parse_timerange_param(req: &Request, res: &mut Response) -> Result<Option<TimeRange>, ()> {
    match req.query::<String>("timerange").filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => match s.parse::<TimeRange>() {
            Ok(tr) => Ok(Some(tr)),
            Err(_) => {
                AppError::bad_request(format!("Invalid timerange: {s}")).write_to(res);
                Err(())
            }
        },
    }
}

/// GET /flows/{flowId}/segments
#[handler]
pub async fn get_segments(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    if !crate::extract::validate_query_params(
        req,
        &[
            "timerange",
            "object_id",
            "reverse_order",
            "presigned",
            "accept_get_urls",
            "accept_storage_ids",
            "include_object_timerange",
            "verbose_storage",
            "limit",
            "page",
        ],
        res,
    ) {
        return;
    }
    let store = get_store(depot);
    let fid = flow_id(req);

    let timerange = match parse_timerange_param(req, res) {
        Ok(tr) => tr,
        Err(()) => return,
    };

    let query = SegmentQuery {
        timerange,
        object_id: req.query::<String>("object_id"),
        reverse_order: req.query::<String>("reverse_order").as_deref() == Some("true"),
        presigned: req.query::<String>("presigned").map(|s| s == "true"),
        accept_get_urls: req.query::<String>("accept_get_urls").map(|s| {
            if s.is_empty() {
                Vec::new()
            } else {
                s.split(',').map(|l| l.trim().to_string()).collect()
            }
        }),
        accept_storage_ids: req.query::<String>("accept_storage_ids").and_then(|s| {
            if s.is_empty() {
                None
            } else {
                Some(s.split(',').map(|l| l.trim().to_string()).collect())
            }
        }),
        include_object_timerange: req.query::<String>("include_object_timerange").as_deref()
            == Some("true"),
        verbose_storage: req.query::<String>("verbose_storage").as_deref() == Some("true"),
    };

    let pagination = pagination_from_request(req);

    let (all, data_timerange) = store.get_segments(&fid, &query).await;

    // Set segment-specific paging headers
    if !data_timerange.is_never() {
        res.add_header("x-paging-timerange", data_timerange.to_string(), true)
            .ok();
    }
    res.add_header("x-paging-count", all.len().to_string(), true)
        .ok();
    res.add_header(
        "x-paging-reverse-order",
        query.reverse_order.to_string(),
        true,
    )
    .ok();

    let page = paginate_and_set_headers(&all, &pagination, req, res);
    res.render(Json(page));
}

/// POST /flows/{flowId}/segments
#[handler]
pub async fn post_segments(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let fid = flow_id(req);

    // Parse body — accepts single object or array
    let body: serde_json::Value = match parse_json(req, res).await {
        Some(v) => v,
        None => return,
    };

    let docs: Vec<serde_json::Value> = if body.is_array() {
        body.as_array().unwrap().clone()
    } else if body.is_object() {
        vec![body]
    } else {
        AppError::bad_request("Body must be a JSON object or array").write_to(res);
        return;
    };

    match store.post_segments(&fid, docs).await {
        Ok(SegmentPostResult::AllCreated) => {
            res.status_code(StatusCode::CREATED);
        }
        Ok(SegmentPostResult::PartialFailure(failed)) => {
            let failed_json: Vec<serde_json::Value> = failed
                .into_iter()
                .map(|f| {
                    let mut obj = serde_json::json!({
                        "object_id": f.object_id,
                        "error": {
                            "type": "TAMSError",
                            "summary": f.error,
                            "time": chrono::Utc::now().to_rfc3339()
                        }
                    });
                    if let Some(tr) = f.timerange {
                        obj.as_object_mut()
                            .unwrap()
                            .insert("timerange".into(), serde_json::Value::String(tr));
                    }
                    obj
                })
                .collect();
            res.status_code(StatusCode::OK);
            res.render(Json(serde_json::json!({
                "failed_segments": failed_json
            })));
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

/// DELETE /flows/{flowId}/segments
#[handler]
pub async fn delete_segments(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let fid = flow_id(req);

    let timerange = match parse_timerange_param(req, res) {
        Ok(tr) => tr,
        Err(()) => return,
    };

    let query = SegmentQuery {
        timerange,
        object_id: req.query::<String>("object_id"),
        ..Default::default()
    };

    match store.delete_segments(&fid, &query).await {
        Ok(SegmentDeleteResult::Deleted) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Ok(SegmentDeleteResult::Async(request)) => {
            res.status_code(StatusCode::ACCEPTED);
            res.add_header(
                "Location",
                format!("/flow-delete-requests/{}", request.id),
                true,
            )
            .unwrap();
            res.render(Json(serde_json::json!(*request)));
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

#[cfg(test)]
mod tests {
    use salvo::prelude::*;
    use salvo::test::{ResponseExt, TestClient};

    use crate::test_utils::test_service;

    fn video_flow_json(flow_id: &str, source_id: &str) -> serde_json::Value {
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

    async fn setup_with_flow() -> (tempfile::TempDir, Service, String) {
        let (service, _store, tmp) = test_service().await;
        let flow_id = "f-seg-test";
        // Create the flow first
        let resp = TestClient::put(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth("test", Some("password"))
            .json(&video_flow_json(flow_id, "src-1"))
            .send(&service)
            .await;
        assert!(
            resp.status_code.unwrap().is_success(),
            "Failed to create flow"
        );
        (tmp, service, flow_id.to_string())
    }

    // -- GET segments --

    #[tokio::test]
    async fn get_segments_empty_flow_returns_empty_list() {
        let (service, _store, _tmp) = test_service().await;
        let mut resp = TestClient::get("http://localhost:5800/flows/nonexistent/segments")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn get_segments_returns_posted_segments() {
        let (_tmp, service, flow_id) = setup_with_flow().await;

        // Post a segment
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        let resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201);

        // Get segments
        let mut resp = TestClient::get(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 1);
        assert_eq!(body[0]["object_id"], "obj-1");
        assert_eq!(body[0]["timerange"], "[0:0_10:0)");
        // Should have get_urls
        assert!(body[0]["get_urls"].is_array());
    }

    // -- POST segments --

    #[tokio::test]
    async fn post_single_segment_returns_201() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        let resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201);

        // Verify segment was actually stored
        let mut resp = TestClient::get(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 1);
        assert_eq!(body[0]["object_id"], "obj-1");
        assert_eq!(body[0]["timerange"], "[0:0_10:0)");
    }

    #[tokio::test]
    async fn post_array_of_segments_returns_201() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let segs = serde_json::json!([
            {"object_id": "obj-1", "timerange": "[0:0_10:0)"},
            {"object_id": "obj-2", "timerange": "[10:0_20:0)"}
        ]);
        let resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&segs)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201);

        // Verify both segments stored
        let mut resp = TestClient::get(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 2);
        assert_eq!(body[0]["object_id"], "obj-1");
        assert_eq!(body[1]["object_id"], "obj-2");
    }

    #[tokio::test]
    async fn post_overlapping_segment_returns_400() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        // Post first segment
        let seg1 = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg1)
            .send(&service)
            .await;

        // Post overlapping segment
        let seg2 = serde_json::json!({
            "object_id": "obj-2",
            "timerange": "[5:0_15:0)"
        });
        let resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg2)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 400);
    }

    #[tokio::test]
    async fn post_adjacent_segments_ok() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let segs = serde_json::json!([
            {"object_id": "obj-1", "timerange": "[0:0_10:0)"},
            {"object_id": "obj-2", "timerange": "[10:0_20:0)"}
        ]);
        let resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&segs)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 201);
    }

    #[tokio::test]
    async fn post_partial_failure_returns_200_with_failed_segments() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        // Post first segment to create an overlap target
        let seg1 = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg1)
            .send(&service)
            .await;

        // Post batch where one succeeds and one overlaps
        let segs = serde_json::json!([
            {"object_id": "obj-2", "timerange": "[20:0_30:0)"},
            {"object_id": "obj-3", "timerange": "[5:0_15:0)"}
        ]);
        let mut resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&segs)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert!(body["failed_segments"].is_array());
        assert_eq!(body["failed_segments"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn post_segment_missing_timerange_fails() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({"object_id": "obj-1"});
        let resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 400);
    }

    #[tokio::test]
    async fn post_segment_missing_object_id_fails() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({"timerange": "[0:0_10:0)"});
        let resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 400);
    }

    #[tokio::test]
    async fn post_segment_on_nonexistent_flow_returns_404() {
        let (service, _store, _tmp) = test_service().await;
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        let resp = TestClient::post("http://localhost:5800/flows/nonexistent/segments")
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 404);
    }

    #[tokio::test]
    async fn post_segment_on_readonly_flow_returns_403() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        // Set read_only
        TestClient::put(format!("http://localhost:5800/flows/{flow_id}/read_only"))
            .basic_auth("test", Some("password"))
            .json(&true)
            .send(&service)
            .await;

        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        let resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 403);
    }

    #[tokio::test]
    async fn post_segment_updates_flow_timerange() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;

        // Check flow now has timerange
        let mut resp = TestClient::get(format!(
            "http://localhost:5800/flows/{flow_id}?include_timerange=true"
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["timerange"], "[0:0_10:0)");
    }

    #[tokio::test]
    async fn post_segment_updates_flow_segments_updated() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;

        let mut resp = TestClient::get(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert!(body["segments_updated"].is_string());
    }

    // -- GET segments filtering --

    #[tokio::test]
    async fn get_segments_timerange_filter() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let segs = serde_json::json!([
            {"object_id": "obj-1", "timerange": "[0:0_10:0)"},
            {"object_id": "obj-2", "timerange": "[10:0_20:0)"},
            {"object_id": "obj-3", "timerange": "[20:0_30:0)"}
        ]);
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&segs)
            .send(&service)
            .await;

        // Filter to get only middle segment
        let mut resp = TestClient::get(format!(
            "http://localhost:5800/flows/{flow_id}/segments?timerange=[10:0_20:0)"
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 1);
        assert_eq!(body[0]["object_id"], "obj-2");
    }

    #[tokio::test]
    async fn get_segments_object_id_filter() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let segs = serde_json::json!([
            {"object_id": "obj-1", "timerange": "[0:0_10:0)"},
            {"object_id": "obj-2", "timerange": "[10:0_20:0)"}
        ]);
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&segs)
            .send(&service)
            .await;

        let mut resp = TestClient::get(format!(
            "http://localhost:5800/flows/{flow_id}/segments?object_id=obj-2"
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 1);
        assert_eq!(body[0]["object_id"], "obj-2");
    }

    #[tokio::test]
    async fn get_segments_reverse_order() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let segs = serde_json::json!([
            {"object_id": "obj-1", "timerange": "[0:0_10:0)"},
            {"object_id": "obj-2", "timerange": "[10:0_20:0)"}
        ]);
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&segs)
            .send(&service)
            .await;

        let mut resp = TestClient::get(format!(
            "http://localhost:5800/flows/{flow_id}/segments?reverse_order=true"
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 2);
        assert_eq!(body[0]["object_id"], "obj-2");
        assert_eq!(body[1]["object_id"], "obj-1");
    }

    #[tokio::test]
    async fn get_segments_has_paging_headers() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;

        let resp = TestClient::get(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let headers = resp.headers();
        assert_eq!(
            headers.get("x-paging-count").unwrap().to_str().unwrap(),
            "1"
        );
        assert_eq!(
            headers.get("x-paging-timerange").unwrap().to_str().unwrap(),
            "[0:0_10:0)"
        );
        assert_eq!(
            headers
                .get("x-paging-reverse-order")
                .unwrap()
                .to_str()
                .unwrap(),
            "false"
        );
        assert!(headers.get("x-paging-limit").is_some());
    }

    #[tokio::test]
    async fn get_segments_x_paging_timerange_is_data_range() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let segs = serde_json::json!([
            {"object_id": "obj-1", "timerange": "[0:0_10:0)"},
            {"object_id": "obj-2", "timerange": "[10:0_20:0)"}
        ]);
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&segs)
            .send(&service)
            .await;

        let resp = TestClient::get(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        let tr = resp
            .headers()
            .get("x-paging-timerange")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(tr, "[0:0_20:0)");
    }

    // -- DELETE segments --

    #[tokio::test]
    async fn delete_all_segments() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;

        let resp = TestClient::delete(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 204);

        // Verify empty
        let mut resp = TestClient::get(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn delete_segments_by_timerange() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let segs = serde_json::json!([
            {"object_id": "obj-1", "timerange": "[0:0_10:0)"},
            {"object_id": "obj-2", "timerange": "[10:0_20:0)"},
            {"object_id": "obj-3", "timerange": "[20:0_30:0)"}
        ]);
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&segs)
            .send(&service)
            .await;

        // Delete first two segments
        let resp = TestClient::delete(format!(
            "http://localhost:5800/flows/{flow_id}/segments?timerange=[0:0_20:0)"
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        assert_eq!(resp.status_code.unwrap(), 204);

        // Only third should remain
        let mut resp = TestClient::get(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 1);
        assert_eq!(body[0]["object_id"], "obj-3");
    }

    #[tokio::test]
    async fn delete_segments_updates_flow_timerange() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let segs = serde_json::json!([
            {"object_id": "obj-1", "timerange": "[0:0_10:0)"},
            {"object_id": "obj-2", "timerange": "[10:0_20:0)"}
        ]);
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&segs)
            .send(&service)
            .await;

        // Delete all segments
        TestClient::delete(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;

        // Flow timerange should be gone
        let mut resp = TestClient::get(format!(
            "http://localhost:5800/flows/{flow_id}?include_timerange=true"
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(
            body["timerange"].as_str(),
            Some("()"),
            "timerange should be '()' after all segments deleted"
        );
    }

    #[tokio::test]
    async fn delete_segments_on_readonly_flow_returns_403() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        TestClient::put(format!("http://localhost:5800/flows/{flow_id}/read_only"))
            .basic_auth("test", Some("password"))
            .json(&true)
            .send(&service)
            .await;

        let resp = TestClient::delete(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 403);
    }

    #[tokio::test]
    async fn head_segments_returns_headers_no_body() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;

        let resp = TestClient::head(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        assert!(resp.headers().get("x-paging-count").is_some());
    }

    // -- Overlap edge cases --

    #[tokio::test]
    async fn overlap_within_batch_rejected() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let segs = serde_json::json!([
            {"object_id": "obj-1", "timerange": "[0:0_10:0)"},
            {"object_id": "obj-2", "timerange": "[5:0_15:0)"}
        ]);
        let mut resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&segs)
            .send(&service)
            .await;
        // Partial failure — first succeeds, second overlaps
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["failed_segments"].as_array().unwrap().len(), 1);
    }

    // -- Pagination --

    #[tokio::test]
    async fn segments_pagination() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        // Create 3 segments
        let segs = serde_json::json!([
            {"object_id": "obj-1", "timerange": "[0:0_10:0)"},
            {"object_id": "obj-2", "timerange": "[10:0_20:0)"},
            {"object_id": "obj-3", "timerange": "[20:0_30:0)"}
        ]);
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&segs)
            .send(&service)
            .await;

        // Get with limit=2
        let mut resp = TestClient::get(format!(
            "http://localhost:5800/flows/{flow_id}/segments?limit=2"
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 2);
        // Should have Link header for next page
        assert!(resp.headers().get("link").is_some());
        assert!(resp.headers().get("x-paging-nextkey").is_some());
    }

    // -- Container validation --

    fn video_flow_no_container(flow_id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": flow_id,
            "source_id": "src-no-container",
            "format": "urn:x-nmos:format:video",
            "codec": "video/h264",
            "essence_parameters": {
                "frame_width": 1920,
                "frame_height": 1080
            }
        })
    }

    #[tokio::test]
    async fn post_segment_no_container_returns_400() {
        let (service, _store, _tmp) = test_service().await;
        let flow_id = "f-seg-no-cont";
        TestClient::put(format!("http://localhost:5800/flows/{flow_id}"))
            .basic_auth("test", Some("password"))
            .json(&video_flow_no_container(flow_id))
            .send(&service)
            .await;

        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        let resp = TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 400);
    }

    // -- DELETE covers semantics --

    #[tokio::test]
    async fn delete_partially_overlapping_not_deleted() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let segs = serde_json::json!([
            {"object_id": "obj-1", "timerange": "[0:0_10:0)"},
            {"object_id": "obj-2", "timerange": "[10:0_20:0)"},
            {"object_id": "obj-3", "timerange": "[20:0_30:0)"}
        ]);
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&segs)
            .send(&service)
            .await;

        // Delete with timerange that partially overlaps first and third,
        // but fully covers only the second.
        let resp = TestClient::delete(format!(
            "http://localhost:5800/flows/{flow_id}/segments?timerange=[5:0_25:0)"
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        assert_eq!(resp.status_code.unwrap(), 204);

        // Only middle segment deleted; first and third remain
        let mut resp = TestClient::get(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 2);
        assert_eq!(body[0]["object_id"], "obj-1");
        assert_eq!(body[1]["object_id"], "obj-3");
    }

    // -- Query parameter: presigned --

    #[tokio::test]
    async fn get_segments_presigned_false_keeps_urls() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;

        let mut resp = TestClient::get(format!(
            "http://localhost:5800/flows/{flow_id}/segments?presigned=false"
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 1);
        // Our URLs are presigned, so presigned=false should exclude them
        let urls = body[0]["get_urls"].as_array().unwrap();
        assert!(urls.is_empty());
    }

    #[tokio::test]
    async fn get_segments_presigned_true_filters_urls() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;

        let mut resp = TestClient::get(format!(
            "http://localhost:5800/flows/{flow_id}/segments?presigned=true"
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 1);
        // Our URLs are presigned (contain access_token), so they should be included
        let urls = body[0]["get_urls"].as_array().unwrap();
        assert_eq!(urls.len(), 1);
    }

    // -- Query parameter: accept_get_urls --

    #[tokio::test]
    async fn get_segments_accept_get_urls_matching() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;

        // Our backend label is "s3"
        let mut resp = TestClient::get(format!(
            "http://localhost:5800/flows/{flow_id}/segments?accept_get_urls=s3"
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        let urls = body[0]["get_urls"].as_array().unwrap();
        assert_eq!(urls.len(), 1);
    }

    #[tokio::test]
    async fn get_segments_accept_get_urls_nonmatching() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;

        // Request a label that doesn't match
        let mut resp = TestClient::get(format!(
            "http://localhost:5800/flows/{flow_id}/segments?accept_get_urls=s3-bucket"
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        let urls = body[0]["get_urls"].as_array().unwrap();
        assert!(urls.is_empty());
    }

    #[tokio::test]
    async fn get_segments_accept_get_urls_empty_removes_urls() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;

        // Empty accept_get_urls = empty get_urls
        let mut resp = TestClient::get(format!(
            "http://localhost:5800/flows/{flow_id}/segments?accept_get_urls="
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        let urls = body[0]["get_urls"].as_array().unwrap();
        assert!(urls.is_empty());
    }

    // -- Query parameter: accept_storage_ids --

    #[tokio::test]
    async fn get_segments_accept_storage_ids_nonmatching() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;

        // Non-matching storage_id filters out our URLs
        let mut resp = TestClient::get(format!(
            "http://localhost:5800/flows/{flow_id}/segments?accept_storage_ids=00000000-0000-0000-0000-000000000000"
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        let urls = body[0]["get_urls"].as_array().unwrap();
        assert!(urls.is_empty());
    }

    // -- Query parameter: include_object_timerange --

    #[tokio::test]
    async fn get_segments_include_object_timerange() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        // Post segment with explicit object_timerange
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)",
            "object_timerange": "[0:0_15:0)"
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;

        // Without include_object_timerange, it should be stripped
        let mut resp = TestClient::get(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(body[0].get("object_timerange").is_none());

        // With include_object_timerange=true, it should be present
        let mut resp = TestClient::get(format!(
            "http://localhost:5800/flows/{flow_id}/segments?include_object_timerange=true"
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body[0]["object_timerange"], "[0:0_15:0)");
    }

    // -- Query parameter: verbose_storage --

    #[tokio::test]
    async fn get_segments_verbose_storage() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;

        // Default (verbose_storage=false): only url, presigned, label
        let mut resp = TestClient::get(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        let url_obj = &body[0]["get_urls"][0];
        assert!(url_obj.get("url").is_some());
        assert!(url_obj.get("presigned").is_some());
        assert!(url_obj.get("label").is_some());
        assert!(url_obj.get("storage_id").is_none());
        assert!(url_obj.get("controlled").is_none());

        // verbose_storage=true: includes storage_id and controlled
        let mut resp = TestClient::get(format!(
            "http://localhost:5800/flows/{flow_id}/segments?verbose_storage=true"
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        let url_obj = &body[0]["get_urls"][0];
        assert!(url_obj.get("url").is_some());
        assert!(url_obj.get("presigned").is_some());
        assert!(url_obj.get("label").is_some());
        assert!(url_obj.get("storage_id").is_some());
        assert!(url_obj.get("controlled").is_some());
        assert_eq!(url_obj["controlled"], true);
    }

    // -- Query parameter: accept_storage_ids (positive case) --

    #[tokio::test]
    async fn get_segments_accept_storage_ids_matching() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)"
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;

        // Get storage backends to find our backend's storage_id
        let mut resp = TestClient::get("http://localhost:5800/service/storage-backends")
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        let backends: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        let storage_id = backends[0]["id"].as_str().unwrap();

        // Use the correct storage_id — URLs should remain
        let mut resp = TestClient::get(format!(
            "http://localhost:5800/flows/{flow_id}/segments?accept_storage_ids={storage_id}"
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        let urls = body[0]["get_urls"].as_array().unwrap();
        assert_eq!(urls.len(), 1);
        assert!(urls[0]["url"]
            .as_str()
            .unwrap()
            .contains(&format!("/{}/", tams_store::TEST_S3_BUCKET)));
    }

    // -- Segment POST with pass-through fields --

    #[tokio::test]
    async fn post_segment_preserves_passthrough_fields() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)",
            "key_frame_count": 5,
            "ts_offset": "0:0",
            "last_duration": {"numerator": 1001, "denominator": 30000}
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;

        let mut resp = TestClient::get(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body[0]["key_frame_count"], 5);
        assert_eq!(body[0]["ts_offset"], "0:0");
        assert_eq!(body[0]["last_duration"]["numerator"], 1001);
    }

    // -- Segment POST with client-provided get_urls (uncontrolled) --

    #[tokio::test]
    async fn post_segment_merges_uncontrolled_get_urls() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)",
            "get_urls": [{
                "url": "https://cdn.example.com/obj-1.ts",
                "controlled": false,
                "label": "cdn"
            }]
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;

        let mut resp = TestClient::get(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .send(&service)
            .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        let urls = body[0]["get_urls"].as_array().unwrap();
        // Should have both: the uncontrolled CDN URL and the server-generated S3 URL
        assert_eq!(urls.len(), 2);
        let labels: Vec<&str> = urls.iter().filter_map(|u| u["label"].as_str()).collect();
        assert!(labels.contains(&"cdn"));
        assert!(labels.contains(&"s3"));
    }

    #[tokio::test]
    async fn get_segments_accept_get_urls_filters_uncontrolled() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)",
            "get_urls": [
                { "url": "https://cdn.example.com/obj-1.ts", "controlled": false, "label": "cdn" },
                { "url": "https://backup.example.com/obj-1.ts", "controlled": false, "label": "backup" }
            ]
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;

        // Filter to cdn only — should exclude backup uncontrolled URL
        let mut resp = TestClient::get(format!(
            "http://localhost:5800/flows/{flow_id}/segments?accept_get_urls=cdn"
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        let urls = body[0]["get_urls"].as_array().unwrap();
        // Only cdn uncontrolled URL should remain (local controlled also excluded by label filter)
        assert_eq!(urls.len(), 1, "expected only cdn URL, got {urls:?}");
        assert_eq!(urls[0]["label"].as_str().unwrap(), "cdn");
    }

    #[tokio::test]
    async fn get_segments_presigned_true_excludes_uncontrolled() {
        let (_tmp, service, flow_id) = setup_with_flow().await;
        let seg = serde_json::json!({
            "object_id": "obj-1",
            "timerange": "[0:0_10:0)",
            "get_urls": [{
                "url": "https://cdn.example.com/obj-1.ts",
                "controlled": false,
                "label": "cdn"
            }]
        });
        TestClient::post(format!("http://localhost:5800/flows/{flow_id}/segments"))
            .basic_auth("test", Some("password"))
            .json(&seg)
            .send(&service)
            .await;

        // presigned=true should exclude uncontrolled URLs
        let mut resp = TestClient::get(format!(
            "http://localhost:5800/flows/{flow_id}/segments?presigned=true"
        ))
        .basic_auth("test", Some("password"))
        .send(&service)
        .await;
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        let urls = body[0]["get_urls"].as_array().unwrap();
        // Only the controlled (S3 presigned) URL should remain
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0]["label"].as_str().unwrap(), "s3");
    }
}

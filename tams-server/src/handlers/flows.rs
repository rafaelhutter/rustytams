use salvo::http::StatusCode;
use salvo::prelude::*;

use crate::error::AppError;
use crate::extract::tag_filters_from_request;
use crate::extract::{paginate_and_set_headers, pagination_from_request};
use crate::handlers::{flow_id, get_store, parse_json};
use tams_types::flow::{DeleteResult, FlowFilters};
use tams_types::timerange::TimeRange;

/// GET /flows
#[handler]
pub async fn get_flows(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    if !crate::extract::validate_query_params(
        req,
        &[
            "source_id",
            "format",
            "codec",
            "label",
            "frame_width",
            "frame_height",
            "timerange",
            "include_timerange",
            "limit",
            "page",
        ],
        res,
    ) {
        return;
    }
    let store = get_store(depot);

    // Validate timerange query param (empty string = no filter)
    let timerange_str = req.query::<String>("timerange").filter(|s| !s.is_empty());
    if let Some(ref tr_str) = timerange_str {
        if tr_str.parse::<TimeRange>().is_err() {
            AppError::bad_request(format!("Invalid timerange: {tr_str}")).write_to(res);
            return;
        }
    }

    let filters = FlowFilters {
        source_id: req.query::<String>("source_id"),
        format: req.query::<String>("format"),
        codec: req.query::<String>("codec"),
        label: req.query::<String>("label"),
        frame_width: req.query::<i64>("frame_width"),
        frame_height: req.query::<i64>("frame_height"),
        timerange: timerange_str,
    };
    let tag_filters = tag_filters_from_request(req);
    let pagination = pagination_from_request(req);
    let include_timerange = req.query::<String>("include_timerange").as_deref() == Some("true");

    let all = store
        .list_flows(&filters, &tag_filters, include_timerange)
        .await;
    let page = paginate_and_set_headers(&all, &pagination, req, res);
    res.render(Json(page));
}

/// GET /flows/{flowId}
#[handler]
pub async fn get_flow(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);
    let include_timerange = req.query::<String>("include_timerange").as_deref() == Some("true");
    let query_timerange: Option<TimeRange> = req
        .query::<String>("timerange")
        .and_then(|s| s.parse::<TimeRange>().ok());

    match store.get_flow(&id).await {
        Some(mut doc) => {
            if !include_timerange {
                if let Some(obj) = doc.as_object_mut() {
                    obj.remove("timerange");
                }
            } else {
                // Include timerange, optionally clipped by query timerange
                if let Some(obj) = doc.as_object_mut() {
                    let flow_tr_str = obj.get("timerange").and_then(|v| v.as_str());
                    let clipped = match (flow_tr_str, &query_timerange) {
                        (Some(fts), Some(qtr)) if !qtr.is_eternity() => {
                            // Clip: intersection of flow timerange and query timerange
                            if let Ok(ftr) = fts.parse::<TimeRange>() {
                                if ftr.overlaps(qtr) {
                                    // Compute intersection
                                    let intersected = ftr.intersect(qtr);
                                    Some(serde_json::Value::String(intersected.to_string()))
                                } else {
                                    Some(serde_json::Value::String("()".to_string()))
                                }
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    if let Some(val) = clipped {
                        obj.insert("timerange".into(), val);
                    } else {
                        // No clipping — use flow's timerange or "()" (never) if absent
                        obj.entry("timerange")
                            .or_insert_with(|| serde_json::Value::String("()".to_string()));
                    }
                }
            }
            res.render(Json(doc));
        }
        None => AppError::not_found(format!("Flow {id} not found")).write_to(res),
    }
}

/// PUT /flows/{flowId}
#[handler]
pub async fn put_flow(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let path_id = flow_id(req);

    let Some(mut doc) = parse_json::<serde_json::Value>(req, res).await else {
        return;
    };

    // Validate body id matches path id
    let body_id = match doc.get("id").and_then(|v| v.as_str()) {
        Some(id) if id == path_id => id.to_string(),
        Some(_) => {
            AppError::bad_request("Body id must match path flowId").write_to(res);
            return;
        }
        None => {
            // id is required by schema but client omitted it — inject from path
            doc.as_object_mut()
                .unwrap()
                .insert("id".into(), serde_json::Value::String(path_id.clone()));
            path_id.clone()
        }
    };
    let _ = body_id;

    match store.put_flow(doc).await {
        Ok((true, Some(flow_doc))) => {
            res.status_code(StatusCode::CREATED);
            res.render(Json(flow_doc));
        }
        Ok((false, _)) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Ok((true, None)) => {
            // Should not happen — create always returns a doc
            res.status_code(StatusCode::CREATED);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

/// DELETE /flows/{flowId}
#[handler]
pub async fn delete_flow(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = flow_id(req);

    match store.delete_flow(&id).await {
        Ok(DeleteResult::Deleted) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Ok(DeleteResult::NotFound) => {
            AppError::not_found(format!("Flow {id} not found")).write_to(res);
        }
        Ok(DeleteResult::Async(deletion_request)) => {
            res.status_code(StatusCode::ACCEPTED);
            res.add_header(
                "location",
                format!("/flow-delete-requests/{}", deletion_request.id),
                true,
            )
            .ok();
            res.render(Json(deletion_request));
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::test_service;
    use salvo::http::StatusCode;
    use salvo::test::{ResponseExt, TestClient};

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
            "label": "Test Flow",
            "description": "A test video flow",
            "generation": 0,
            "tags": {"genre": "news"},
            "essence_parameters": {
                "frame_rate": {"numerator": 25, "denominator": 1},
                "frame_width": 1920,
                "frame_height": 1080,
                "bit_depth": 8,
                "interlace_mode": "progressive",
                "component_type": "YCbCr",
                "horiz_chroma_subs": 2,
                "vert_chroma_subs": 2
            }
        })
    }

    fn audio_flow_json(id: &str, source_id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "source_id": source_id,
            "format": "urn:x-nmos:format:audio",
            "codec": "audio/aac",
            "essence_parameters": {
                "sample_rate": 48000,
                "channels": 2
            }
        })
    }

    fn multi_flow_json(id: &str, source_id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "source_id": source_id,
            "format": "urn:x-nmos:format:multi"
        })
    }

    fn image_flow_json(id: &str, source_id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "source_id": source_id,
            "format": "urn:x-nmos:format:image",
            "codec": "image/jpeg",
            "essence_parameters": {
                "frame_width": 3840,
                "frame_height": 2160
            }
        })
    }

    fn data_flow_json(id: &str, source_id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "source_id": source_id,
            "format": "urn:x-nmos:format:data"
        })
    }

    const FLOW_ID: &str = "aaaaaaaa-bbbb-1ccc-9ddd-eeeeeeeeeeee";
    const SOURCE_ID: &str = "11111111-2222-3333-4444-555555555555";
    const FLOW_ID_2: &str = "aaaaaaaa-bbbb-1ccc-9ddd-ffffffffffff";

    // ========== PUT /flows/{flowId} ==========

    #[tokio::test]
    async fn put_flow_creates_new_returns_201() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(flow["id"], FLOW_ID);
        assert_eq!(flow["source_id"], SOURCE_ID);
        assert_eq!(flow["format"], "urn:x-nmos:format:video");
    }

    #[tokio::test]
    async fn put_flow_update_returns_204() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        // First PUT creates
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);
        // Second PUT updates
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn put_flow_id_mismatch_returns_400() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json("wrong-id-in-body", SOURCE_ID);
        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::BAD_REQUEST);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert!(body["type"].is_string());
        assert!(body["summary"].is_string());
        assert!(body["time"].is_string());
    }

    #[tokio::test]
    async fn put_flow_missing_source_id_returns_400() {
        let (svc, _store, _tmp) = test_service().await;
        let body = serde_json::json!({
            "id": FLOW_ID,
            "format": "urn:x-nmos:format:video"
        });
        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::BAD_REQUEST);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert!(body["type"].is_string());
        assert!(body["summary"].is_string());
        assert!(body["time"].is_string());
    }

    #[tokio::test]
    async fn put_flow_missing_format_returns_400() {
        let (svc, _store, _tmp) = test_service().await;
        let body = serde_json::json!({
            "id": FLOW_ID,
            "source_id": SOURCE_ID
        });
        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::BAD_REQUEST);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert!(body["type"].is_string());
        assert!(body["summary"].is_string());
        assert!(body["time"].is_string());
    }

    #[tokio::test]
    async fn put_flow_invalid_json_returns_400() {
        let (svc, _store, _tmp) = test_service().await;
        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .raw_json("not valid json")
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::BAD_REQUEST);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert!(body["type"].is_string());
        assert!(body["summary"].is_string());
        assert!(body["time"].is_string());
    }

    #[tokio::test]
    async fn put_flow_server_managed_fields_ignored() {
        let (svc, _store, _tmp) = test_service().await;
        let mut body = video_flow_json(FLOW_ID, SOURCE_ID);
        // Client sends server-managed fields -- should be ignored
        body["created"] = serde_json::json!("1999-01-01T00:00:00Z");
        body["metadata_updated"] = serde_json::json!("1999-01-01T00:00:00Z");
        body["segments_updated"] = serde_json::json!("1999-01-01T00:00:00Z");
        body["timerange"] = serde_json::json!("[0:0_10:0)");
        body["collected_by"] = serde_json::json!(["fake-id"]);

        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        // Server-managed fields should NOT be the client's values
        assert_ne!(flow["created"], "1999-01-01T00:00:00Z");
        assert!(flow.get("timerange").is_none());
        assert!(
            flow.get("collected_by").is_none()
                || flow["collected_by"]
                    .as_array()
                    .map(|a| a.is_empty())
                    .unwrap_or(true)
        );
    }

    #[tokio::test]
    async fn put_flow_preserves_client_fields() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(flow["codec"], "video/h264");
        assert_eq!(flow["container"], "video/mp2t");
        assert_eq!(flow["label"], "Test Flow");
        assert_eq!(flow["generation"], 0);
        assert_eq!(flow["essence_parameters"]["frame_width"], 1920);
        assert_eq!(flow["essence_parameters"]["frame_height"], 1080);
    }

    #[tokio::test]
    async fn put_flow_auto_creates_source() {
        let (svc, store, _tmp) = test_service().await;
        // Source doesn't exist yet
        assert!(store.get_source(SOURCE_ID).await.is_none());
        // PUT flow
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);
        // Source should now exist
        let source = store.get_source(SOURCE_ID).await.unwrap();
        assert_eq!(source.id, SOURCE_ID);
        assert_eq!(source.format, "urn:x-nmos:format:video");
    }

    #[tokio::test]
    async fn put_flow_read_only_returns_403() {
        let (svc, _store, _tmp) = test_service().await;
        let mut body = video_flow_json(FLOW_ID, SOURCE_ID);
        body["read_only"] = serde_json::json!(true);
        // Create the flow first
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);
        // Now try to update it -- should be rejected
        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::FORBIDDEN);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert!(body["type"].is_string());
        assert!(body["summary"].is_string());
        assert!(body["time"].is_string());
    }

    // ========== GET /flows/{flowId} ==========

    #[tokio::test]
    async fn get_flow_returns_200() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(flow["id"], FLOW_ID);
        assert_eq!(flow["format"], "urn:x-nmos:format:video");
    }

    #[tokio::test]
    async fn get_flow_not_found() {
        let (svc, _store, _tmp) = test_service().await;
        let resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_flow_omits_timerange_by_default() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        assert!(flow.get("timerange").is_none());
    }

    #[tokio::test]
    async fn get_flow_includes_timerange_when_requested() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        let mut resp = TestClient::get(format!(
            "http://localhost/flows/{FLOW_ID}?include_timerange=true"
        ))
        .basic_auth(auth().0, auth().1)
        .send(&svc)
        .await;
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        // timerange should be "()" (never) when no segments exist
        assert_eq!(
            flow["timerange"].as_str(),
            Some("()"),
            "timerange should be '()' (never) when no segments exist"
        );
    }

    // ========== HEAD /flows, HEAD /flows/{flowId} ==========

    #[tokio::test]
    async fn head_flows_returns_ok_no_body() {
        let (svc, _store, _tmp) = test_service().await;
        let mut resp = TestClient::head("http://localhost/flows")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        assert!(resp.take_string().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn head_flow_returns_ok_no_body() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        let mut resp = TestClient::head(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        assert!(resp.take_string().await.unwrap().is_empty());
    }

    // ========== GET /flows (list + filtering) ==========

    #[tokio::test]
    async fn get_flows_empty() {
        let (svc, _store, _tmp) = test_service().await;
        let mut resp = TestClient::get("http://localhost/flows")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn get_flows_returns_created_flow() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        let mut resp = TestClient::get("http://localhost/flows")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(flows.len(), 1);
        assert_eq!(flows[0]["id"], FLOW_ID);
    }

    #[tokio::test]
    async fn get_flows_list_omits_timerange_by_default() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        let mut resp = TestClient::get("http://localhost/flows")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(flows[0].get("timerange").is_none());
    }

    #[tokio::test]
    async fn get_flows_list_includes_timerange_when_requested() {
        let (svc, store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        // Add a segment so the flow has a timerange
        store
            .post_segments(
                FLOW_ID,
                vec![serde_json::json!({
                    "object_id": "obj-1",
                    "timerange": "[0:0_5:0)",
                })],
            )
            .await
            .unwrap();

        // Without include_timerange: no timerange in response
        let mut resp = TestClient::get("http://localhost/flows")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(flows[0].get("timerange").is_none());

        // With include_timerange=true: timerange present
        let mut resp = TestClient::get("http://localhost/flows?include_timerange=true")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(flows[0].get("timerange").is_some());
        let tr = flows[0]["timerange"].as_str().unwrap();
        assert!(tr.contains("0:0"), "timerange should contain start: {tr}");
    }

    #[tokio::test]
    async fn get_flows_filter_by_source_id() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        // Match
        let mut resp = TestClient::get(format!("http://localhost/flows?source_id={SOURCE_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(flows.len(), 1);

        // No match
        let mut resp = TestClient::get(
            "http://localhost/flows?source_id=00000000-0000-0000-0000-000000000000",
        )
        .basic_auth(auth().0, auth().1)
        .send(&svc)
        .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(flows.is_empty());
    }

    #[tokio::test]
    async fn get_flows_filter_by_format() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        let mut resp = TestClient::get("http://localhost/flows?format=urn:x-nmos:format:video")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(flows.len(), 1);

        let mut resp = TestClient::get("http://localhost/flows?format=urn:x-nmos:format:audio")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(flows.is_empty());
    }

    #[tokio::test]
    async fn get_flows_filter_by_codec() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        let mut resp = TestClient::get("http://localhost/flows?codec=video/h264")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(flows.len(), 1);

        let mut resp = TestClient::get("http://localhost/flows?codec=audio/aac")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(flows.is_empty());
    }

    #[tokio::test]
    async fn get_flows_filter_by_label() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        let mut resp = TestClient::get("http://localhost/flows?label=Test+Flow")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(flows.len(), 1);

        let mut resp = TestClient::get("http://localhost/flows?label=Nonexistent")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(flows.is_empty());
    }

    #[tokio::test]
    async fn get_flows_filter_by_tag() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        let mut resp = TestClient::get("http://localhost/flows?tag.genre=news")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(flows.len(), 1);

        let mut resp = TestClient::get("http://localhost/flows?tag.genre=sport")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(flows.is_empty());
    }

    #[tokio::test]
    async fn get_flows_filter_by_frame_width() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        let mut resp = TestClient::get("http://localhost/flows?frame_width=1920")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(flows.len(), 1);

        let mut resp = TestClient::get("http://localhost/flows?frame_width=1280")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(flows.is_empty());
    }

    #[tokio::test]
    async fn get_flows_filter_by_frame_height() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        let mut resp = TestClient::get("http://localhost/flows?frame_height=1080")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(flows.len(), 1);

        let mut resp = TestClient::get("http://localhost/flows?frame_height=720")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(flows.is_empty());
    }

    #[tokio::test]
    async fn get_flows_frame_filter_excludes_audio() {
        let (svc, _store, _tmp) = test_service().await;
        // Create an audio flow (no frame_width/height)
        let body = audio_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        let mut resp = TestClient::get("http://localhost/flows?frame_width=1920")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(
            flows.is_empty(),
            "Audio flow should not match frame_width filter"
        );
    }

    // ========== GET /flows pagination ==========

    #[tokio::test]
    async fn get_flows_pagination() {
        let (svc, _store, _tmp) = test_service().await;
        // Create 3 flows
        for i in 0..3 {
            let fid = format!("aaaaaaaa-bbbb-1ccc-9ddd-{i:012}");
            let sid = format!("11111111-2222-3333-4444-{i:012}");
            let body = video_flow_json(&fid, &sid);
            TestClient::put(format!("http://localhost/flows/{fid}"))
                .basic_auth(auth().0, auth().1)
                .json(&body)
                .send(&svc)
                .await;
        }

        // Page 1: limit=2
        let mut resp = TestClient::get("http://localhost/flows?limit=2")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let page1: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(page1.len(), 2);
        assert!(resp.headers().get("link").is_some());
        assert!(resp.headers().get("x-paging-nextkey").is_some());

        // Page 2: use next key
        let next_key = resp
            .headers()
            .get("x-paging-nextkey")
            .unwrap()
            .to_str()
            .unwrap();
        let mut resp = TestClient::get(format!("http://localhost/flows?limit=2&page={next_key}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let page2: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(page2.len(), 1);
        // Last page: no link
        assert!(resp.headers().get("link").is_none());
    }

    // ========== DELETE /flows/{flowId} ==========

    #[tokio::test]
    async fn delete_flow_returns_204() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        let resp = TestClient::delete(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        // Verify gone
        let resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delete_flow_not_found() {
        let (svc, _store, _tmp) = test_service().await;
        let resp = TestClient::delete(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delete_flow_read_only_returns_403() {
        let (svc, _store, _tmp) = test_service().await;
        let mut body = video_flow_json(FLOW_ID, SOURCE_ID);
        body["read_only"] = serde_json::json!(true);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        let mut resp = TestClient::delete(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::FORBIDDEN);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert!(body["type"].is_string());
        assert!(body["summary"].is_string());
        assert!(body["time"].is_string());
    }

    // ========== Flow polymorphism ==========

    #[tokio::test]
    async fn put_audio_flow() {
        let (svc, _store, _tmp) = test_service().await;
        let body = audio_flow_json(FLOW_ID, SOURCE_ID);
        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(flow["format"], "urn:x-nmos:format:audio");
        assert_eq!(flow["essence_parameters"]["sample_rate"], 48000);
    }

    #[tokio::test]
    async fn put_multi_flow() {
        let (svc, _store, _tmp) = test_service().await;
        let body = multi_flow_json(FLOW_ID, SOURCE_ID);
        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(flow["format"], "urn:x-nmos:format:multi");
        // Multi flows don't require codec or essence_parameters
        assert!(flow.get("codec").is_none());
    }

    // ========== Cross-endpoint: source auto-creation ==========

    #[tokio::test]
    async fn auto_created_source_visible_via_get_sources() {
        let (svc, store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        // Check via store directly
        let source = store.get_source(SOURCE_ID).await.unwrap();
        assert_eq!(source.format, "urn:x-nmos:format:video");
        assert!(source.created.is_some());

        // Also check via HTTP
        let mut resp = TestClient::get(format!("http://localhost/sources/{SOURCE_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let src: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(src["id"], SOURCE_ID);
        assert_eq!(src["format"], "urn:x-nmos:format:video");
    }

    #[tokio::test]
    async fn put_flow_does_not_duplicate_existing_source() {
        let (svc, store, _tmp) = test_service().await;
        // Create first flow — auto-creates source
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        // Create second flow with same source_id
        let body2 = video_flow_json(FLOW_ID_2, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID_2}"))
            .basic_auth(auth().0, auth().1)
            .json(&body2)
            .send(&svc)
            .await;

        // Should still be just one source
        let sources = store
            .list_sources(
                &tams_types::source::SourceFilters::default(),
                &tams_types::tags::TagFilters::default(),
            )
            .await;
        assert_eq!(sources.len(), 1);
    }

    // ========== Server-managed timestamps ==========

    #[tokio::test]
    async fn put_flow_sets_created_and_metadata_updated() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        assert!(flow["created"].is_string());
        assert!(flow["metadata_updated"].is_string());
    }

    // ========== Format validation ==========

    #[tokio::test]
    async fn put_flow_invalid_format_returns_400() {
        let (svc, _store, _tmp) = test_service().await;
        let body = serde_json::json!({
            "id": FLOW_ID,
            "source_id": SOURCE_ID,
            "format": "garbage"
        });
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn put_flow_video_missing_codec_returns_400() {
        let (svc, _store, _tmp) = test_service().await;
        let body = serde_json::json!({
            "id": FLOW_ID,
            "source_id": SOURCE_ID,
            "format": "urn:x-nmos:format:video",
            "essence_parameters": {
                "frame_width": 1920,
                "frame_height": 1080
            }
        });
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn put_flow_video_missing_essence_parameters_returns_400() {
        let (svc, _store, _tmp) = test_service().await;
        let body = serde_json::json!({
            "id": FLOW_ID,
            "source_id": SOURCE_ID,
            "format": "urn:x-nmos:format:video",
            "codec": "video/h264"
        });
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn put_flow_video_missing_frame_width_returns_400() {
        let (svc, _store, _tmp) = test_service().await;
        let body = serde_json::json!({
            "id": FLOW_ID,
            "source_id": SOURCE_ID,
            "format": "urn:x-nmos:format:video",
            "codec": "video/h264",
            "essence_parameters": {
                "frame_height": 1080
            }
        });
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn put_flow_audio_missing_codec_returns_400() {
        let (svc, _store, _tmp) = test_service().await;
        let body = serde_json::json!({
            "id": FLOW_ID,
            "source_id": SOURCE_ID,
            "format": "urn:x-nmos:format:audio",
            "essence_parameters": {
                "sample_rate": 48000,
                "channels": 2
            }
        });
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::BAD_REQUEST);
    }

    // ========== Image and data flow polymorphism ==========

    #[tokio::test]
    async fn put_image_flow() {
        let (svc, _store, _tmp) = test_service().await;
        let body = image_flow_json(FLOW_ID, SOURCE_ID);
        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(flow["format"], "urn:x-nmos:format:image");
        assert_eq!(flow["codec"], "image/jpeg");
        assert_eq!(flow["essence_parameters"]["frame_width"], 3840);
        assert_eq!(flow["essence_parameters"]["frame_height"], 2160);
    }

    #[tokio::test]
    async fn put_data_flow() {
        let (svc, _store, _tmp) = test_service().await;
        let body = data_flow_json(FLOW_ID, SOURCE_ID);
        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(flow["format"], "urn:x-nmos:format:data");
        // Data flows don't require codec or essence_parameters
        assert!(flow.get("codec").is_none());
        assert!(flow.get("essence_parameters").is_none());
    }

    // ========== metadata_version ==========

    #[tokio::test]
    async fn put_flow_sets_metadata_version() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        assert!(
            flow["metadata_version"].is_string(),
            "metadata_version should be a non-null string"
        );
        assert!(
            !flow["metadata_version"].as_str().unwrap().is_empty(),
            "metadata_version should not be empty"
        );
    }

    #[tokio::test]
    async fn put_flow_update_changes_metadata_version() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        // Create
        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);
        let flow1: serde_json::Value = resp.take_json().await.unwrap();
        let ver1 = flow1["metadata_version"].as_str().unwrap().to_string();

        // Update -- fetch the doc back so we can verify the new version
        let _resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        // GET the flow to read back the updated metadata_version
        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flow2: serde_json::Value = resp.take_json().await.unwrap();
        let ver2 = flow2["metadata_version"].as_str().unwrap().to_string();

        assert_ne!(ver1, ver2, "metadata_version should change on update");
    }

    #[tokio::test]
    async fn put_flow_client_metadata_version_ignored() {
        let (svc, _store, _tmp) = test_service().await;
        let mut body = video_flow_json(FLOW_ID, SOURCE_ID);
        body["metadata_version"] = serde_json::json!("client-provided-version");

        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        assert_ne!(
            flow["metadata_version"].as_str().unwrap(),
            "client-provided-version",
            "Server should override client-supplied metadata_version"
        );
    }

    // ========== created_by / updated_by ==========

    #[tokio::test]
    async fn put_flow_sets_created_by() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        assert!(
            flow["created_by"].is_string(),
            "created_by should be set on creation"
        );
        assert_eq!(flow["created_by"], "server");
        assert_eq!(flow["updated_by"], "server");
    }

    #[tokio::test]
    async fn put_flow_update_preserves_created_by() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        // Create
        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);
        let flow1: serde_json::Value = resp.take_json().await.unwrap();
        let created_by = flow1["created_by"].as_str().unwrap().to_string();

        // Update
        let _resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        // GET to verify
        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flow2: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(
            flow2["created_by"].as_str().unwrap(),
            created_by,
            "created_by should be preserved across updates"
        );
        assert_eq!(flow2["updated_by"], "server");
    }

    // ========== F15: tag_exists filter on flows ==========

    #[tokio::test]
    async fn get_flows_filter_tag_exists() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        // tag_exists.genre=true should match (flow has genre tag)
        let mut resp = TestClient::get("http://localhost/flows?tag_exists.genre=true")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(flows.len(), 1);

        // tag_exists.genre=false should NOT match
        let mut resp = TestClient::get("http://localhost/flows?tag_exists.genre=false")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(flows.is_empty());

        // tag_exists.nonexistent=true should NOT match
        let mut resp = TestClient::get("http://localhost/flows?tag_exists.nonexistent=true")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flows: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(flows.is_empty());
    }

    // ========== F18: generation non-zero + preserved across update ==========

    #[tokio::test]
    async fn put_flow_generation_nonzero_preserved() {
        let (svc, _store, _tmp) = test_service().await;
        let mut body = video_flow_json(FLOW_ID, SOURCE_ID);
        body["generation"] = serde_json::json!(5);
        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(flow["generation"], 5, "generation=5 must round-trip");

        // Update preserves generation
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(flow["generation"], 5, "generation must survive update");
    }

    // ========== F19: 204 response body must be empty ==========

    #[tokio::test]
    async fn put_flow_update_204_has_no_body() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);
        let text = resp.take_string().await.unwrap();
        assert!(
            text.is_empty(),
            "204 response must have no body, got: {text}"
        );
    }

    #[tokio::test]
    async fn delete_flow_204_has_no_body() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;

        let mut resp = TestClient::delete(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);
        let text = resp.take_string().await.unwrap();
        assert!(
            text.is_empty(),
            "204 response must have no body, got: {text}"
        );
    }

    // ========== read_only flip ==========

    #[tokio::test]
    async fn put_flow_read_only_can_be_flipped_back_to_false() {
        let (svc, _store, _tmp) = test_service().await;
        let mut body = video_flow_json(FLOW_ID, SOURCE_ID);
        body["read_only"] = serde_json::json!(true);
        // Create with read_only=true
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::CREATED);

        // PUT with read_only=false should succeed (flip back)
        body["read_only"] = serde_json::json!(false);
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        // Subsequent updates should now work
        body["label"] = serde_json::json!("Updated Label");
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);
    }

    // ========== malformed tags rejection ==========

    #[tokio::test]
    async fn put_flow_malformed_tags_returns_400() {
        let (svc, _store, _tmp) = test_service().await;
        let mut body = video_flow_json(FLOW_ID, SOURCE_ID);
        // Tags must be {string: string | string[]}, not {string: number}
        body["tags"] = serde_json::json!({"genre": 42});
        let mut resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::BAD_REQUEST);
        let err: serde_json::Value = resp.take_json().await.unwrap();
        assert!(err["summary"].as_str().unwrap().contains("tags"));
    }

    // ========== Timerange filter on GET /flows ==========

    #[tokio::test]
    async fn get_flows_filter_by_timerange_excludes_no_timerange() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        // Flow has no segments so no timerange — specific timerange filter should exclude it
        let mut resp = TestClient::get("http://localhost/flows?timerange=[0:0_10:0)")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(
            body.is_empty(),
            "Flow with no timerange should be excluded by specific timerange filter"
        );
    }

    #[tokio::test]
    async fn get_flows_default_timerange_returns_all() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        // Default (no timerange param) should return all flows
        let mut resp = TestClient::get("http://localhost/flows")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(
            body.len(),
            1,
            "Default (no timerange filter) should return all flows"
        );
    }

    #[tokio::test]
    async fn get_flows_empty_timerange_returns_flows_without_content() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        // "()" = empty/never: returns flows with no content (no segments)
        let mut resp = TestClient::get("http://localhost/flows?timerange=()")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(
            body.len(),
            1,
            "Empty timerange filter should return flows with no content"
        );
    }

    #[tokio::test]
    async fn get_flows_timerange_filter_overlapping() {
        // Set up a flow with a known timerange via test helper
        let (svc, store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        // Simulate this flow having segments covering [0:0_20:0)
        store.test_set_flow_timerange(FLOW_ID, "[0:0_20:0)").await;
        // Query with overlapping timerange should include the flow
        let mut resp = TestClient::get("http://localhost/flows?timerange=[5:0_15:0)")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 1, "Overlapping timerange should match");
    }

    #[tokio::test]
    async fn get_flows_timerange_filter_non_overlapping() {
        let (svc, store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        store.test_set_flow_timerange(FLOW_ID, "[0:0_10:0)").await;
        // Query with non-overlapping timerange should exclude the flow
        let mut resp = TestClient::get("http://localhost/flows?timerange=[20:0_30:0)")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(
            body.is_empty(),
            "Non-overlapping timerange should exclude flow"
        );
    }

    #[tokio::test]
    async fn get_flows_eternity_timerange_includes_all() {
        let (svc, store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        store.test_set_flow_timerange(FLOW_ID, "[0:0_10:0)").await;
        // Eternity should include all flows
        let mut resp = TestClient::get("http://localhost/flows?timerange=_")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 1, "Eternity timerange should include all flows");
    }

    // ========== Timerange clipping on GET /flows/{flowId} ==========

    #[tokio::test]
    async fn get_flow_timerange_clipping() {
        let (svc, store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        // Flow has timerange [0:0_20:0)
        store.test_set_flow_timerange(FLOW_ID, "[0:0_20:0)").await;
        // Request with include_timerange=true and timerange=[5:0_15:0) should clip
        let mut resp = TestClient::get(format!(
            "http://localhost/flows/{FLOW_ID}?include_timerange=true&timerange=[5:0_15:0)"
        ))
        .basic_auth(auth().0, auth().1)
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        let tr = flow["timerange"]
            .as_str()
            .expect("timerange should be a string");
        assert_eq!(
            tr, "[5:0_15:0)",
            "Timerange should be clipped to intersection"
        );
    }

    #[tokio::test]
    async fn get_flow_timerange_no_overlap_returns_never() {
        let (svc, store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        store.test_set_flow_timerange(FLOW_ID, "[0:0_10:0)").await;
        // Query timerange doesn't overlap flow timerange
        let mut resp = TestClient::get(format!(
            "http://localhost/flows/{FLOW_ID}?include_timerange=true&timerange=[20:0_30:0)"
        ))
        .basic_auth(auth().0, auth().1)
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(
            flow["timerange"].as_str().unwrap(),
            "()",
            "Non-overlapping timerange should return never"
        );
    }

    #[tokio::test]
    async fn get_flow_timerange_no_clip_without_query() {
        let (svc, store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        store.test_set_flow_timerange(FLOW_ID, "[0:0_20:0)").await;
        // include_timerange=true without timerange query → full timerange
        let mut resp = TestClient::get(format!(
            "http://localhost/flows/{FLOW_ID}?include_timerange=true"
        ))
        .basic_auth(auth().0, auth().1)
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(
            flow["timerange"].as_str().unwrap(),
            "[0:0_20:0)",
            "Without timerange query, full timerange should be returned"
        );
    }

    // ========== DELETE 202 + Location ==========

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn delete_flow_with_segments_returns_202_with_location() {
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        // Allocate storage and create a real segment
        let mut resp = TestClient::post(format!("http://localhost/flows/{FLOW_ID}/storage"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!({"limit": 1}))
            .send(&svc)
            .await;
        let storage: serde_json::Value = resp.take_json().await.unwrap();
        let oid = storage["media_objects"][0]["object_id"].as_str().unwrap();
        TestClient::post(format!("http://localhost/flows/{FLOW_ID}/segments"))
            .basic_auth(auth().0, auth().1)
            .json(&serde_json::json!([{
                "object_id": oid,
                "timerange": "[0:0_10:0)"
            }]))
            .send(&svc)
            .await;

        let mut resp = TestClient::delete(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::ACCEPTED);
        // Must have Location header pointing to deletion request
        let location = resp
            .headers()
            .get("location")
            .expect("202 must have Location header")
            .to_str()
            .unwrap();
        assert!(
            location.starts_with("/flow-delete-requests/"),
            "Location should point to flow-delete-requests, got: {location}"
        );
        // Body should be a deletion request
        let dr: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(dr["flow_id"], FLOW_ID);
        assert_eq!(dr["delete_flow"], true);
        assert_eq!(dr["status"], "created");
        assert!(dr["timerange_to_delete"].is_string());
    }

    #[tokio::test]
    async fn delete_flow_without_segments_returns_204() {
        // Existing behavior: flow with no segments → 204
        let (svc, _store, _tmp) = test_service().await;
        let body = video_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        let resp = TestClient::delete(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);
        assert!(
            resp.headers().get("location").is_none(),
            "204 should not have Location header"
        );
    }

    // ========== FlowCore new fields pass-through ==========

    #[tokio::test]
    async fn put_flow_preserves_max_bit_rate() {
        let (svc, _store, _tmp) = test_service().await;
        let mut body = video_flow_json(FLOW_ID, SOURCE_ID);
        body["max_bit_rate"] = serde_json::json!(5000);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(flow["max_bit_rate"], 5000);
    }

    #[tokio::test]
    async fn put_flow_preserves_avg_bit_rate() {
        let (svc, _store, _tmp) = test_service().await;
        let mut body = video_flow_json(FLOW_ID, SOURCE_ID);
        body["avg_bit_rate"] = serde_json::json!(3000);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(flow["avg_bit_rate"], 3000);
    }

    #[tokio::test]
    async fn put_flow_preserves_flow_collection() {
        let (svc, _store, _tmp) = test_service().await;
        let mut body = multi_flow_json(FLOW_ID, SOURCE_ID);
        body["flow_collection"] = serde_json::json!([
            {"id": "aaaaaaaa-bbbb-1ccc-9ddd-111111111111", "role": "video"},
            {"id": "aaaaaaaa-bbbb-1ccc-9ddd-222222222222", "role": "audio"}
        ]);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&body)
            .send(&svc)
            .await;
        let mut resp = TestClient::get(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let flow: serde_json::Value = resp.take_json().await.unwrap();
        assert!(flow["flow_collection"].is_array());
        assert_eq!(flow["flow_collection"].as_array().unwrap().len(), 2);
        assert_eq!(flow["flow_collection"][0]["role"], "video");
    }

    #[tokio::test]
    async fn set_flow_collection_syncs_source_collection() {
        let (svc, _store, _tmp) = test_service().await;
        // Create parent flow (multi format)
        let parent = multi_flow_json(FLOW_ID, SOURCE_ID);
        TestClient::put(format!("http://localhost/flows/{FLOW_ID}"))
            .basic_auth(auth().0, auth().1)
            .json(&parent)
            .send(&svc)
            .await;

        // Create child video flow with its own source
        let child_source_id = "cccccccc-dddd-1eee-9fff-111111111111";
        let child_flow_id = "cccccccc-dddd-1eee-9fff-222222222222";
        let mut child = video_flow_json(child_flow_id, child_source_id);
        child["container"] = serde_json::json!("video/mp2t");
        TestClient::put(format!("http://localhost/flows/{child_flow_id}"))
            .basic_auth(auth().0, auth().1)
            .json(&child)
            .send(&svc)
            .await;

        // Set flow_collection on parent
        let collection = serde_json::json!([
            {"id": child_flow_id, "role": "video"}
        ]);
        let resp = TestClient::put(format!("http://localhost/flows/{FLOW_ID}/flow_collection"))
            .basic_auth(auth().0, auth().1)
            .json(&collection)
            .send(&svc)
            .await;
        assert_eq!(
            resp.status_code.unwrap(),
            salvo::http::StatusCode::NO_CONTENT
        );

        // Parent source should now have source_collection
        let mut resp = TestClient::get(format!("http://localhost/sources/{SOURCE_ID}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let source: serde_json::Value = resp.take_json().await.unwrap();
        let sc = source["source_collection"].as_array().unwrap();
        assert_eq!(sc.len(), 1);
        assert_eq!(sc[0]["id"].as_str().unwrap(), child_source_id);
        assert_eq!(sc[0]["role"].as_str().unwrap(), "video");

        // Child source should have collected_by pointing to parent source
        let mut resp = TestClient::get(format!("http://localhost/sources/{child_source_id}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        let child_source: serde_json::Value = resp.take_json().await.unwrap();
        let cb = child_source["collected_by"].as_array().unwrap();
        assert!(cb.iter().any(|v| v.as_str() == Some(SOURCE_ID)));
    }
}

use salvo::http::StatusCode;
use salvo::prelude::*;

use crate::error::AppError;
use crate::extract::tag_filters_from_request;
use crate::extract::{paginate_and_set_headers, pagination_from_request};
use crate::handlers::{get_store, parse_json, tag_name};
use tams_types::source::SourceFilters;
use tams_store::DeleteResult;

fn source_id(req: &Request) -> String {
    req.param::<String>("sourceId")
        .expect("sourceId path param missing from route")
}

/// GET /sources -- list sources with optional filtering and pagination.
#[handler]
pub async fn get_sources(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    if !crate::extract::validate_query_params(req, &["label", "format", "limit", "page"], res) {
        return;
    }
    let store = get_store(depot);
    let filters = SourceFilters {
        label: req.query::<String>("label"),
        format: req.query::<String>("format"),
    };
    let tag_filters = tag_filters_from_request(req);
    let pagination = pagination_from_request(req);

    let all = store.list_sources(&filters, &tag_filters).await;
    let page = paginate_and_set_headers(&all, &pagination, req, res);
    res.render(Json(page));
}

/// GET /sources/{sourceId} -- get a single source.
#[handler]
pub async fn get_source(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = source_id(req);
    match store.get_source(&id).await {
        Some(source) => res.render(Json(source)),
        None => AppError::not_found(format!("Source {id} not found")).write_to(res),
    }
}

// -- Tags --

/// GET /sources/{sourceId}/tags -- get all tags for a source.
#[handler]
pub async fn get_source_tags(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = source_id(req);
    match store.get_source_tags(&id).await {
        Some(tags) => res.render(Json(tags)),
        None => AppError::not_found(format!("Source {id} not found")).write_to(res),
    }
}

/// GET /sources/{sourceId}/tags/{name} -- get a single tag value.
#[handler]
pub async fn get_source_tag(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = source_id(req);
    let name = tag_name(req);
    match store.get_source_tag(&id, &name).await {
        Some(value) => res.render(Json(value)),
        None => AppError::not_found(format!("Tag {name} not found on source {id}")).write_to(res),
    }
}

/// PUT /sources/{sourceId}/tags/{name} -- set a tag value.
#[handler]
pub async fn put_source_tag(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = source_id(req);
    let name = tag_name(req);
    let Some(value) = parse_json::<tams_types::tags::TagValue>(req, res).await else {
        return;
    };
    match store.set_source_tag(&id, &name, value).await {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

/// DELETE /sources/{sourceId}/tags/{name} -- delete a tag.
#[handler]
pub async fn delete_source_tag(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = source_id(req);
    let name = tag_name(req);
    match store.delete_source_tag(&id, &name).await {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

// -- Label --

/// GET /sources/{sourceId}/label
#[handler]
pub async fn get_source_label(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = source_id(req);
    match store.get_source_label(&id).await {
        Some(Some(label)) => res.render(Json(label)),
        Some(None) => AppError::not_found(format!("Label not set on source {id}")).write_to(res),
        None => AppError::not_found(format!("Source {id} not found")).write_to(res),
    }
}

/// PUT /sources/{sourceId}/label
#[handler]
pub async fn put_source_label(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = source_id(req);
    let Some(label) = parse_json::<String>(req, res).await else {
        return;
    };
    match store.set_source_label(&id, label).await {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

/// DELETE /sources/{sourceId}/label
#[handler]
pub async fn delete_source_label(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = source_id(req);
    match store.delete_source_label(&id).await {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

// -- Description --

/// GET /sources/{sourceId}/description
#[handler]
pub async fn get_source_description(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = source_id(req);
    match store.get_source_description(&id).await {
        Some(Some(desc)) => res.render(Json(desc)),
        Some(None) => {
            AppError::not_found(format!("Description not set on source {id}")).write_to(res)
        }
        None => AppError::not_found(format!("Source {id} not found")).write_to(res),
    }
}

/// PUT /sources/{sourceId}/description
#[handler]
pub async fn put_source_description(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = source_id(req);
    let Some(desc) = parse_json::<String>(req, res).await else {
        return;
    };
    match store.set_source_description(&id, desc).await {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

/// DELETE /sources/{sourceId}/description
#[handler]
pub async fn delete_source_description(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = source_id(req);
    match store.delete_source_description(&id).await {
        Ok(()) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Err(e) => AppError::from(e).write_to(res),
    }
}

/// DELETE /sources/{sourceId} -- delete a source and all its flows.
#[handler]
pub async fn delete_source(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let store = get_store(depot);
    let id = source_id(req);
    match store.delete_source(&id).await {
        Ok(DeleteResult::Deleted) => {
            res.status_code(StatusCode::NO_CONTENT);
        }
        Ok(DeleteResult::NotFound) => {
            AppError::not_found(format!("Source {id} not found")).write_to(res);
        }
        Ok(DeleteResult::Async(_)) => unreachable!("delete_source never returns Async"),
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
    use tams_types::source::Source;
    use tams_types::tags::{TagValue, Tags};

    async fn setup_with_source() -> (Service, tempfile::TempDir) {
        let (svc, store, tmp) = test_service().await;
        store
            .create_source(Source {
                id: "6b640f2d-27e3-4440-9b6e-0e1f76e4e928".into(),
                format: "urn:x-nmos:format:video".into(),
                label: Some("Test Source".into()),
                description: Some("A test source".into()),
                tags: Some({
                    let mut t = Tags::new();
                    t.insert("genre".into(), TagValue::Single("news".into()));
                    t
                }),
                created_by: None,
                updated_by: None,
                created: None,
                updated: None,
                source_collection: None,
                collected_by: None,
            })
            .await
            .unwrap();
        (svc, tmp)
    }

    async fn setup_empty() -> (Service, tempfile::TempDir) {
        let (svc, _store, tmp) = test_service().await;
        (svc, tmp)
    }

    /// Create a service with multiple sources for pagination and filtering tests.
    async fn setup_multi() -> (Service, Store, tempfile::TempDir) {
        let (svc, store, tmp) = test_service().await;
        for i in 0..5 {
            let mut tags = Tags::new();
            tags.insert("genre".into(), TagValue::Single(format!("genre-{i}")));
            tags.insert(
                "topics".into(),
                TagValue::Multiple(vec!["news".into(), "sport".into()]),
            );
            store
                .create_source(Source {
                    id: format!("00000000-0000-0000-0000-00000000000{i}"),
                    format: "urn:x-nmos:format:video".into(),
                    label: Some(format!("Source {i}")),
                    description: Some(format!("Description {i}")),
                    tags: Some(tags),
                    created_by: None,
                    updated_by: None,
                    created: None,
                    updated: None,
                    source_collection: None,
                    collected_by: None,
                })
                .await
                .unwrap();
        }
        (svc, store, tmp)
    }

    fn auth() -> (&'static str, Option<&'static str>) {
        ("test", Some("password"))
    }

    // -- GET /sources --

    #[tokio::test]
    async fn get_sources_empty() {
        let (svc, _tmp) = setup_empty().await;
        let mut resp = TestClient::get("http://localhost/sources")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn get_sources_returns_source() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp = TestClient::get("http://localhost/sources")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 1);
        assert_eq!(body[0]["id"], "6b640f2d-27e3-4440-9b6e-0e1f76e4e928");
    }

    #[tokio::test]
    async fn get_sources_filter_by_label() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp = TestClient::get("http://localhost/sources?label=Test+Source")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 1);
    }

    #[tokio::test]
    async fn get_sources_filter_by_label_no_match() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp = TestClient::get("http://localhost/sources?label=Nonexistent")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn get_sources_filter_by_format() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp = TestClient::get("http://localhost/sources?format=urn:x-nmos:format:video")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 1);
    }

    #[tokio::test]
    async fn get_sources_filter_by_format_no_match() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp = TestClient::get("http://localhost/sources?format=urn:x-nmos:format:audio")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn get_sources_filter_by_tag() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp = TestClient::get("http://localhost/sources?tag.genre=news")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 1);
    }

    #[tokio::test]
    async fn get_sources_filter_by_tag_no_match() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp = TestClient::get("http://localhost/sources?tag.genre=sport")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn get_sources_filter_tag_exists() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp = TestClient::get("http://localhost/sources?tag_exists.genre=true")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 1);
    }

    #[tokio::test]
    async fn get_sources_filter_tag_not_exists() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp = TestClient::get("http://localhost/sources?tag_exists.category=true")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn get_sources_pagination_headers() {
        let (svc, _tmp) = setup_with_source().await;
        let resp = TestClient::get("http://localhost/sources?limit=10")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        // Should have X-Paging-Limit header
        let limit = resp.headers().get("x-paging-limit");
        assert!(limit.is_some());
        assert_eq!(limit.unwrap().to_str().unwrap(), "10");
    }

    #[tokio::test]
    async fn get_sources_pagination_no_next_on_last_page() {
        let (svc, _tmp) = setup_with_source().await;
        let resp = TestClient::get("http://localhost/sources?limit=100")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        // Only 1 source, should NOT have Link header
        assert!(resp.headers().get("link").is_none());
    }

    // -- GET /sources/{sourceId} --

    #[tokio::test]
    async fn get_source_found() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp =
            TestClient::get("http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928")
                .basic_auth(auth().0, auth().1)
                .send(&svc)
                .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["id"], "6b640f2d-27e3-4440-9b6e-0e1f76e4e928");
        assert_eq!(body["format"], "urn:x-nmos:format:video");
    }

    #[tokio::test]
    async fn get_source_not_found() {
        let (svc, _tmp) = setup_empty().await;
        let resp = TestClient::get("http://localhost/sources/00000000-0000-0000-0000-000000000000")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    // -- HEAD /sources --

    #[tokio::test]
    async fn head_sources_returns_ok_no_body() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp = TestClient::head("http://localhost/sources")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body = resp.take_string().await.unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn head_source_returns_ok_no_body() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp =
            TestClient::head("http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928")
                .basic_auth(auth().0, auth().1)
                .send(&svc)
                .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body = resp.take_string().await.unwrap();
        assert!(body.is_empty());
    }

    // -- Tags CRUD --

    #[tokio::test]
    async fn get_source_tags() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp =
            TestClient::get("http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/tags")
                .basic_auth(auth().0, auth().1)
                .send(&svc)
                .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["genre"], "news");
    }

    #[tokio::test]
    async fn get_source_tag_found() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp = TestClient::get(
            "http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/tags/genre",
        )
        .basic_auth(auth().0, auth().1)
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body, "news");
    }

    #[tokio::test]
    async fn get_source_tag_not_found() {
        let (svc, _tmp) = setup_with_source().await;
        let resp = TestClient::get(
            "http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/tags/nonexistent",
        )
        .basic_auth(auth().0, auth().1)
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn put_source_tag_creates() {
        let (svc, _tmp) = setup_with_source().await;
        let resp = TestClient::put(
            "http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/tags/category",
        )
        .basic_auth(auth().0, auth().1)
        .json(&serde_json::json!("sports"))
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        // Verify it was created
        let mut resp = TestClient::get(
            "http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/tags/category",
        )
        .basic_auth(auth().0, auth().1)
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body, "sports");
    }

    #[tokio::test]
    async fn delete_source_tag() {
        let (svc, _tmp) = setup_with_source().await;
        let resp = TestClient::delete(
            "http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/tags/genre",
        )
        .basic_auth(auth().0, auth().1)
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        // Verify it was deleted
        let resp = TestClient::get(
            "http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/tags/genre",
        )
        .basic_auth(auth().0, auth().1)
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    // -- Label CRUD --

    #[tokio::test]
    async fn get_source_label() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp =
            TestClient::get("http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/label")
                .basic_auth(auth().0, auth().1)
                .send(&svc)
                .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body, "Test Source");
    }

    #[tokio::test]
    async fn put_source_label() {
        let (svc, _tmp) = setup_with_source().await;
        let resp =
            TestClient::put("http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/label")
                .basic_auth(auth().0, auth().1)
                .json(&serde_json::json!("New Label"))
                .send(&svc)
                .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        let mut resp =
            TestClient::get("http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/label")
                .basic_auth(auth().0, auth().1)
                .send(&svc)
                .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body, "New Label");
    }

    #[tokio::test]
    async fn delete_source_label() {
        let (svc, _tmp) = setup_with_source().await;
        let resp = TestClient::delete(
            "http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/label",
        )
        .basic_auth(auth().0, auth().1)
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        let resp =
            TestClient::get("http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/label")
                .basic_auth(auth().0, auth().1)
                .send(&svc)
                .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    // -- Description CRUD --

    #[tokio::test]
    async fn get_source_description() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp = TestClient::get(
            "http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/description",
        )
        .basic_auth(auth().0, auth().1)
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body, "A test source");
    }

    #[tokio::test]
    async fn put_source_description() {
        let (svc, _tmp) = setup_with_source().await;
        let resp = TestClient::put(
            "http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/description",
        )
        .basic_auth(auth().0, auth().1)
        .json(&serde_json::json!("Updated description"))
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        let mut resp = TestClient::get(
            "http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/description",
        )
        .basic_auth(auth().0, auth().1)
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body, "Updated description");
    }

    #[tokio::test]
    async fn delete_source_description() {
        let (svc, _tmp) = setup_with_source().await;
        let resp = TestClient::delete(
            "http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/description",
        )
        .basic_auth(auth().0, auth().1)
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        let resp = TestClient::get(
            "http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/description",
        )
        .basic_auth(auth().0, auth().1)
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    // -- Tags/label/description on missing source --

    #[tokio::test]
    async fn tags_on_missing_source_returns_404() {
        let (svc, _tmp) = setup_empty().await;
        let resp =
            TestClient::get("http://localhost/sources/00000000-0000-0000-0000-000000000000/tags")
                .basic_auth(auth().0, auth().1)
                .send(&svc)
                .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn label_on_missing_source_returns_404() {
        let (svc, _tmp) = setup_empty().await;
        let resp =
            TestClient::get("http://localhost/sources/00000000-0000-0000-0000-000000000000/label")
                .basic_auth(auth().0, auth().1)
                .send(&svc)
                .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn description_on_missing_source_returns_404() {
        let (svc, _tmp) = setup_empty().await;
        let resp = TestClient::get(
            "http://localhost/sources/00000000-0000-0000-0000-000000000000/description",
        )
        .basic_auth(auth().0, auth().1)
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    // -- Invalid JSON --

    #[tokio::test]
    async fn put_source_tag_invalid_json() {
        let (svc, _tmp) = setup_with_source().await;
        let resp = TestClient::put(
            "http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/tags/genre",
        )
        .basic_auth(auth().0, auth().1)
        .raw_json("not valid json")
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn put_source_label_invalid_json() {
        let (svc, _tmp) = setup_with_source().await;
        let resp =
            TestClient::put("http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/label")
                .basic_auth(auth().0, auth().1)
                .raw_json("not valid json")
                .send(&svc)
                .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn put_source_description_invalid_json() {
        let (svc, _tmp) = setup_with_source().await;
        let resp = TestClient::put(
            "http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/description",
        )
        .basic_auth(auth().0, auth().1)
        .raw_json("not valid json")
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::BAD_REQUEST);
    }

    // -- HEAD on sub-resources --

    #[tokio::test]
    async fn head_source_tags_no_body() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp =
            TestClient::head("http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/tags")
                .basic_auth(auth().0, auth().1)
                .send(&svc)
                .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        assert!(resp.take_string().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn head_source_tag_no_body() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp = TestClient::head(
            "http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/tags/genre",
        )
        .basic_auth(auth().0, auth().1)
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        assert!(resp.take_string().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn head_source_label_no_body() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp =
            TestClient::head("http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/label")
                .basic_auth(auth().0, auth().1)
                .send(&svc)
                .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        assert!(resp.take_string().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn head_source_description_no_body() {
        let (svc, _tmp) = setup_with_source().await;
        let mut resp = TestClient::head(
            "http://localhost/sources/6b640f2d-27e3-4440-9b6e-0e1f76e4e928/description",
        )
        .basic_auth(auth().0, auth().1)
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        assert!(resp.take_string().await.unwrap().is_empty());
    }

    // -- Tag filtering: OR with comma-separated values --

    #[tokio::test]
    async fn get_sources_tag_or_filter() {
        let (svc, _store, _tmp) = setup_multi().await;
        // genre-0 or genre-1 should match 2 sources
        let mut resp = TestClient::get("http://localhost/sources?tag.genre=genre-0,genre-1")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 2);
    }

    // -- tag_exists=false --

    #[tokio::test]
    async fn get_sources_tag_exists_false() {
        let (svc, _store, _tmp) = setup_multi().await;
        // All sources have "genre" tag, so tag_exists.genre=false should return 0
        let mut resp = TestClient::get("http://localhost/sources?tag_exists.genre=false")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(body.is_empty());
    }

    // -- Multiple filters AND semantics --

    #[tokio::test]
    async fn get_sources_combined_filters_and() {
        let (svc, _store, _tmp) = setup_multi().await;
        // label=Source 0 AND tag.genre=genre-0 should match exactly 1
        let mut resp = TestClient::get("http://localhost/sources?label=Source+0&tag.genre=genre-0")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 1);
        assert_eq!(body[0]["label"], "Source 0");
    }

    #[tokio::test]
    async fn get_sources_combined_filters_and_no_match() {
        let (svc, _store, _tmp) = setup_multi().await;
        // label=Source 0 AND tag.genre=genre-1 should match 0 (AND fails)
        let mut resp = TestClient::get("http://localhost/sources?label=Source+0&tag.genre=genre-1")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert!(body.is_empty());
    }

    // -- TagValue::Multiple filtering --

    #[tokio::test]
    async fn get_sources_filter_on_multiple_tag_value() {
        let (svc, _store, _tmp) = setup_multi().await;
        // "topics" tag is Multiple(["news", "sport"]). Filter tag.topics=news should match all 5.
        let mut resp = TestClient::get("http://localhost/sources?tag.topics=news")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(body.len(), 5);
    }

    // -- Pagination: Link + NextKey when more pages --

    #[tokio::test]
    async fn get_sources_pagination_link_header() {
        let (svc, _store, _tmp) = setup_multi().await;
        let resp = TestClient::get("http://localhost/sources?limit=2")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let link = resp.headers().get("link");
        assert!(
            link.is_some(),
            "Link header must be present when more pages exist"
        );
        let link_str = link.unwrap().to_str().unwrap();
        assert!(link_str.contains("rel=\"next\""));
        assert!(resp.headers().get("x-paging-nextkey").is_some());
    }

    // -- Pagination: limit capping --

    #[tokio::test]
    async fn get_sources_pagination_limit_capped() {
        let (svc, _store, _tmp) = setup_multi().await;
        let resp = TestClient::get("http://localhost/sources?limit=9999")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let limit = resp
            .headers()
            .get("x-paging-limit")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(limit, "1000");
    }

    // -- Pagination: page token navigation --

    #[tokio::test]
    async fn get_sources_pagination_page_token() {
        let (svc, _store, _tmp) = setup_multi().await;
        // Get page 1 (2 items)
        let mut resp = TestClient::get("http://localhost/sources?limit=2")
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let page1: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(page1.len(), 2);
        let next_key = resp
            .headers()
            .get("x-paging-nextkey")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        // Get page 2 using the token
        let mut resp = TestClient::get(format!("http://localhost/sources?limit=2&page={next_key}"))
            .basic_auth(auth().0, auth().1)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let page2: Vec<serde_json::Value> = resp.take_json().await.unwrap();
        assert_eq!(page2.len(), 2);
        // Results are sorted by ID, so pages should be in order and non-overlapping
        let ids1: Vec<&str> = page1.iter().map(|s| s["id"].as_str().unwrap()).collect();
        let ids2: Vec<&str> = page2.iter().map(|s| s["id"].as_str().unwrap()).collect();
        assert_eq!(
            ids1,
            &[
                "00000000-0000-0000-0000-000000000000",
                "00000000-0000-0000-0000-000000000001"
            ]
        );
        assert_eq!(
            ids2,
            &[
                "00000000-0000-0000-0000-000000000002",
                "00000000-0000-0000-0000-000000000003"
            ]
        );
    }

    // -- Timestamps --

    #[tokio::test]
    async fn source_creation_sets_timestamps() {
        let (_svc, store, _tmp) = setup_multi().await;
        let source = store
            .get_source("00000000-0000-0000-0000-000000000000")
            .await
            .unwrap();
        let created = source.created.as_ref().expect("created must be set");
        let updated = source.updated.as_ref().expect("updated must be set");
        // Validate RFC 3339 format by parsing
        assert!(
            chrono::DateTime::parse_from_rfc3339(created).is_ok(),
            "created must be valid RFC 3339: {created}"
        );
        assert!(
            chrono::DateTime::parse_from_rfc3339(updated).is_ok(),
            "updated must be valid RFC 3339: {updated}"
        );
    }

    #[tokio::test]
    async fn write_operation_updates_timestamp() {
        let (svc, store, _tmp) = setup_multi().await;
        let before = store
            .get_source("00000000-0000-0000-0000-000000000000")
            .await
            .unwrap()
            .updated
            .unwrap();

        // Perform a write via the API
        let resp = TestClient::put(
            "http://localhost/sources/00000000-0000-0000-0000-000000000000/tags/newtag",
        )
        .basic_auth(auth().0, auth().1)
        .json(&serde_json::json!("value"))
        .send(&svc)
        .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::NO_CONTENT);

        let after = store
            .get_source("00000000-0000-0000-0000-000000000000")
            .await
            .unwrap()
            .updated
            .unwrap();
        assert_ne!(after, before, "updated timestamp must change after write");
    }
}

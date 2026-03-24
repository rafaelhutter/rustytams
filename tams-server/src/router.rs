use std::time::Duration;

use salvo::affix_state;
use salvo::catcher::Catcher;
use salvo::cors::{Any, Cors, CorsHandler};
use salvo::http::Method;
use salvo::prelude::*;

use crate::auth_client::AuthClient;
use crate::auth_middleware;
use crate::handlers;
use tams_store::Store;

/// Build the complete Service (router + JSON error catcher + CORS).
pub fn build_service(store: Store, auth_client: AuthClient) -> Service {
    let router = build_router(store, auth_client);
    Service::new(router)
        .hoop(cors_handler())
        .catcher(Catcher::default().hoop(json_error_catcher))
}

fn cors_handler() -> CorsHandler {
    Cors::new()
        .allow_origin(Any)
        .allow_methods(vec![
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::HEAD,
            Method::OPTIONS,
        ])
        .allow_headers(vec![
            "authorization",
            "content-type",
            "accept",
            "origin",
            "x-requested-with",
        ])
        .expose_headers(vec![
            "link",
            "x-paging-limit",
            "x-paging-nextkey",
            "x-paging-timerange",
            "x-paging-count",
            "x-paging-reverse-order",
            "location",
        ])
        .max_age(Duration::from_secs(3600))
        .into_handler()
}

fn build_router(store: Store, auth_client: AuthClient) -> Router {
    Router::new()
        .hoop(affix_state::inject(store))
        .hoop(affix_state::inject(auth_client))
        // Docs (unauthenticated, embedded at compile time)
        .push(Router::with_path("docs").get(handlers::docs::get_docs))
        .push(Router::with_path("docs/{file}").get(handlers::docs::get_docs_asset))
        .push(Router::with_path("api-spec").get(handlers::docs::get_api_spec))
        .push(
            // /token -- no auth required (this IS the auth endpoint)
            Router::with_path("token").post(auth_middleware::post_token),
        )
        .push(
            // All other routes require auth
            Router::new()
                .hoop(auth_middleware::tams_auth)
                .hoop(head_strip)
                .push(
                    Router::new()
                        .get(handlers::service::get_root)
                        .head(handlers::service::get_root),
                )
                .push(
                    Router::with_path("service")
                        .get(handlers::service::get_service)
                        .head(handlers::service::get_service)
                        .post(handlers::service::post_service),
                )
                .push(
                    Router::with_path("service/storage-backends")
                        .get(handlers::service::get_storage_backends)
                        .head(handlers::service::get_storage_backends),
                )
                // Webhooks
                .push(
                    Router::with_path("service/webhooks")
                        .get(handlers::webhooks::get_webhooks)
                        .head(handlers::webhooks::get_webhooks)
                        .post(handlers::webhooks::post_webhook),
                )
                .push(
                    Router::with_path("service/webhooks/{webhookId}")
                        .get(handlers::webhooks::get_webhook)
                        .head(handlers::webhooks::get_webhook)
                        .put(handlers::webhooks::put_webhook)
                        .delete(handlers::webhooks::delete_webhook),
                )
                // Delete Requests
                .push(
                    Router::with_path("flow-delete-requests")
                        .get(handlers::delete_requests::get_delete_requests)
                        .head(handlers::delete_requests::get_delete_requests),
                )
                .push(
                    Router::with_path("flow-delete-requests/{request-id}")
                        .get(handlers::delete_requests::get_delete_request)
                        .head(handlers::delete_requests::get_delete_request),
                )
                // Sources
                .push(
                    Router::with_path("sources")
                        .get(handlers::sources::get_sources)
                        .head(handlers::sources::get_sources),
                )
                .push(
                    Router::with_path("sources/{sourceId}")
                        .get(handlers::sources::get_source)
                        .head(handlers::sources::get_source),
                )
                .push(
                    Router::with_path("sources/{sourceId}/tags")
                        .get(handlers::sources::get_source_tags)
                        .head(handlers::sources::get_source_tags),
                )
                .push(
                    Router::with_path("sources/{sourceId}/tags/{name}")
                        .get(handlers::sources::get_source_tag)
                        .head(handlers::sources::get_source_tag)
                        .put(handlers::sources::put_source_tag)
                        .delete(handlers::sources::delete_source_tag),
                )
                .push(
                    Router::with_path("sources/{sourceId}/label")
                        .get(handlers::sources::get_source_label)
                        .head(handlers::sources::get_source_label)
                        .put(handlers::sources::put_source_label)
                        .delete(handlers::sources::delete_source_label),
                )
                .push(
                    Router::with_path("sources/{sourceId}/description")
                        .get(handlers::sources::get_source_description)
                        .head(handlers::sources::get_source_description)
                        .put(handlers::sources::put_source_description)
                        .delete(handlers::sources::delete_source_description),
                )
                // Flows
                .push(
                    Router::with_path("flows")
                        .get(handlers::flows::get_flows)
                        .head(handlers::flows::get_flows),
                )
                .push(
                    Router::with_path("flows/{flowId}")
                        .get(handlers::flows::get_flow)
                        .head(handlers::flows::get_flow)
                        .put(handlers::flows::put_flow)
                        .delete(handlers::flows::delete_flow),
                )
                // Flow properties
                .push(
                    Router::with_path("flows/{flowId}/tags")
                        .get(handlers::flow_props::get_flow_tags)
                        .head(handlers::flow_props::get_flow_tags),
                )
                .push(
                    Router::with_path("flows/{flowId}/tags/{name}")
                        .get(handlers::flow_props::get_flow_tag)
                        .head(handlers::flow_props::get_flow_tag)
                        .put(handlers::flow_props::put_flow_tag)
                        .delete(handlers::flow_props::delete_flow_tag),
                )
                .push(
                    Router::with_path("flows/{flowId}/label")
                        .get(handlers::flow_props::get_flow_label)
                        .head(handlers::flow_props::get_flow_label)
                        .put(handlers::flow_props::put_flow_label)
                        .delete(handlers::flow_props::delete_flow_label),
                )
                .push(
                    Router::with_path("flows/{flowId}/description")
                        .get(handlers::flow_props::get_flow_description)
                        .head(handlers::flow_props::get_flow_description)
                        .put(handlers::flow_props::put_flow_description)
                        .delete(handlers::flow_props::delete_flow_description),
                )
                .push(
                    Router::with_path("flows/{flowId}/read_only")
                        .get(handlers::flow_props::get_flow_read_only)
                        .head(handlers::flow_props::get_flow_read_only)
                        .put(handlers::flow_props::put_flow_read_only),
                )
                .push(
                    Router::with_path("flows/{flowId}/flow_collection")
                        .get(handlers::flow_props::get_flow_collection)
                        .head(handlers::flow_props::get_flow_collection)
                        .put(handlers::flow_props::put_flow_collection)
                        .delete(handlers::flow_props::delete_flow_collection),
                )
                .push(
                    Router::with_path("flows/{flowId}/max_bit_rate")
                        .get(handlers::flow_props::get_flow_max_bit_rate)
                        .head(handlers::flow_props::get_flow_max_bit_rate)
                        .put(handlers::flow_props::put_flow_max_bit_rate)
                        .delete(handlers::flow_props::delete_flow_max_bit_rate),
                )
                .push(
                    Router::with_path("flows/{flowId}/avg_bit_rate")
                        .get(handlers::flow_props::get_flow_avg_bit_rate)
                        .head(handlers::flow_props::get_flow_avg_bit_rate)
                        .put(handlers::flow_props::put_flow_avg_bit_rate)
                        .delete(handlers::flow_props::delete_flow_avg_bit_rate),
                )
                // Segments
                .push(
                    Router::with_path("flows/{flowId}/segments")
                        .get(handlers::segments::get_segments)
                        .head(handlers::segments::get_segments)
                        .post(handlers::segments::post_segments)
                        .delete(handlers::segments::delete_segments),
                )
                // Storage
                .push(
                    Router::with_path("flows/{flowId}/storage")
                        .post(handlers::storage::post_storage),
                )
                // Objects
                .push(
                    Router::with_path("objects/{objectId}")
                        .get(handlers::objects::get_object)
                        .head(handlers::objects::get_object),
                )
                .push(
                    Router::with_path("objects/{objectId}/instances")
                        .post(handlers::objects::post_object_instance)
                        .delete(handlers::objects::delete_object_instance),
                ),
        )
}

/// Middleware that strips the response body for HEAD requests.
#[handler]
async fn head_strip(req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
    let is_head = req.method() == salvo::http::Method::HEAD;
    ctrl.call_next(req, depot, res).await;
    if is_head {
        res.take_body();
    }
}

/// Catcher hoop that renders error responses as JSON matching schemas/error.json.
#[handler]
async fn json_error_catcher(res: &mut Response) {
    if let Some(status) = res.status_code {
        if status.is_client_error() || status.is_server_error() {
            let error_type = status
                .canonical_reason()
                .unwrap_or("error")
                .to_lowercase()
                .replace(' ', "_");
            res.render(Json(serde_json::json!({
                "type": error_type,
                "summary": status.to_string(),
                "time": chrono::Utc::now().to_rfc3339()
            })));
        }
    }
}

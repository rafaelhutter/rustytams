use salvo::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use salvo::http::{HeaderValue, StatusCode};
use salvo::prelude::*;

static SWAGGER_HTML: &str = include_str!("../../swagger-ui/index.html");
static SWAGGER_BUNDLE_JS: &str = include_str!("../../swagger-ui/swagger-ui-bundle.js");
static SWAGGER_PRESET_JS: &str = include_str!("../../swagger-ui/swagger-ui-standalone-preset.js");
static SWAGGER_CSS: &str = include_str!("../../swagger-ui/swagger-ui.css");
static SWAGGER_FAVICON: &[u8] = include_bytes!("../../swagger-ui/favicon-32x32.png");
static API_SPEC: &str = include_str!("../../swagger-ui/api-spec.yaml");

/// Vendored assets are version-pinned and never change between builds.
static HV_CACHE_IMMUTABLE: HeaderValue =
    HeaderValue::from_static("public, max-age=86400, immutable");
/// Spec may change between builds — cacheable but revalidatable.
static HV_CACHE_SPEC: HeaderValue = HeaderValue::from_static("public, max-age=3600");
static HV_IMAGE_PNG: HeaderValue = HeaderValue::from_static("image/png");
static HV_APP_YAML: HeaderValue = HeaderValue::from_static("application/x-yaml");

/// GET /docs — Swagger UI HTML page.
#[handler]
pub async fn get_docs(res: &mut Response) {
    res.render(Text::Html(SWAGGER_HTML));
}

/// GET /docs/{file} — Swagger UI static assets (embedded at compile time).
#[handler]
pub async fn get_docs_asset(req: &mut Request, res: &mut Response) {
    let file = req.param::<String>("file").unwrap_or_default();
    match file.as_str() {
        "swagger-ui-bundle.js" => res.render(Text::Js(SWAGGER_BUNDLE_JS)),
        "swagger-ui-standalone-preset.js" => res.render(Text::Js(SWAGGER_PRESET_JS)),
        "swagger-ui.css" => res.render(Text::Css(SWAGGER_CSS)),
        "favicon-32x32.png" => {
            res.headers_mut().insert(CONTENT_TYPE, HV_IMAGE_PNG.clone());
            res.write_body(SWAGGER_FAVICON).ok();
        }
        _ => {
            res.status_code(StatusCode::NOT_FOUND);
            return;
        }
    }
    res.headers_mut()
        .insert(CACHE_CONTROL, HV_CACHE_IMMUTABLE.clone());
}

/// GET /api-spec — resolved OpenAPI YAML specification.
#[handler]
pub async fn get_api_spec(res: &mut Response) {
    res.headers_mut().insert(CONTENT_TYPE, HV_APP_YAML.clone());
    res.headers_mut()
        .insert(CACHE_CONTROL, HV_CACHE_SPEC.clone());
    res.render(Text::Plain(API_SPEC));
}

#[cfg(test)]
mod tests {
    use salvo::test::{ResponseExt, TestClient};

    use crate::test_utils::test_service;

    #[tokio::test]
    async fn get_docs_returns_html() {
        let (service, _store, _tmp) = test_service().await;
        let mut resp = TestClient::get("http://localhost:5800/docs")
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body = resp.take_string().await.unwrap();
        assert!(body.contains("swagger-ui"));
    }

    #[tokio::test]
    async fn get_api_spec_returns_yaml() {
        let (service, _store, _tmp) = test_service().await;
        let mut resp = TestClient::get("http://localhost:5800/api-spec")
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
        let body = resp.take_string().await.unwrap();
        assert!(body.contains("openapi"));
    }

    #[tokio::test]
    async fn get_docs_asset_js() {
        let (service, _store, _tmp) = test_service().await;
        let resp = TestClient::get("http://localhost:5800/docs/swagger-ui-bundle.js")
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
    }

    #[tokio::test]
    async fn get_docs_asset_css() {
        let (service, _store, _tmp) = test_service().await;
        let resp = TestClient::get("http://localhost:5800/docs/swagger-ui.css")
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
    }

    #[tokio::test]
    async fn get_docs_asset_favicon() {
        let (service, _store, _tmp) = test_service().await;
        let resp = TestClient::get("http://localhost:5800/docs/favicon-32x32.png")
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 200);
    }

    #[tokio::test]
    async fn get_docs_asset_unknown_returns_404() {
        let (service, _store, _tmp) = test_service().await;
        let resp = TestClient::get("http://localhost:5800/docs/nonexistent.js")
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), 404);
    }
}

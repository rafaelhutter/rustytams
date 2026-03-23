use salvo::prelude::*;

use crate::auth_client::AuthClient;
use crate::error::AppError;

/// Decode a base64-encoded Basic auth string into (user, pass).
fn decode_basic_auth(encoded: &str) -> Option<(String, String)> {
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let creds = String::from_utf8(decoded).ok()?;
    let (user, pass) = creds.split_once(':')?;
    Some((user.to_string(), pass.to_string()))
}

/// Extract Basic auth credentials from the Authorization header.
fn extract_basic_credentials(req: &Request) -> Option<(String, String)> {
    req.headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Basic "))
        .and_then(decode_basic_auth)
}

/// Extract all auth credentials from the request as JSON values
/// suitable for POSTing to /auth/check.
fn extract_auth_requests(req: &Request) -> Vec<serde_json::Value> {
    let mut requests = Vec::new();

    if let Some(auth_header) = req.headers().get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(encoded) = auth_str.strip_prefix("Basic ") {
                if let Some((user, pass)) = decode_basic_auth(encoded) {
                    requests.push(serde_json::json!({
                        "auth_type": "Basic", "user": user, "pass": pass
                    }));
                }
            }
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                requests.push(serde_json::json!({
                    "auth_type": "Bearer", "token": token
                }));
            }
        }
    }

    if let Some(key) = req.query::<String>("access_token") {
        requests.push(serde_json::json!({
            "auth_type": "ApiKey", "key": key
        }));
    }

    requests
}

/// TamsAuth middleware -- checks Basic, Bearer, and API key auth (OR semantics).
/// Delegates to the external auth-server via HTTP.
#[handler]
pub async fn tams_auth(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
    ctrl: &mut FlowCtrl,
) {
    let auth_client = depot
        .obtain::<AuthClient>()
        .expect("AuthClient not in depot");

    for ar in extract_auth_requests(req) {
        if auth_client.check(&ar).await {
            ctrl.call_next(req, depot, res).await;
            return;
        }
    }

    AppError::unauthorized("No valid authentication credentials provided").write_to(res);
}

/// POST /token -- issue bearer token (non-spec convenience endpoint).
///
/// Expects `Content-Type: application/x-www-form-urlencoded` with `grant_type=client_credentials`.
/// Authenticates via Basic auth header (client_id:client_secret).
/// Forwards to the auth-server's /auth/token endpoint.
#[handler]
pub async fn post_token(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let auth_client = depot
        .obtain::<AuthClient>()
        .expect("AuthClient not in depot");

    // Extract Basic auth
    let (user, pass) = match extract_basic_credentials(req) {
        Some(creds) => creds,
        None => {
            AppError::unauthorized("Invalid client credentials").write_to(res);
            return;
        }
    };

    // Parse form body and validate grant_type
    let form = req.form_data().await;
    let grant_type = form.ok().and_then(|f| f.fields.get("grant_type").cloned());
    if grant_type.as_deref() != Some("client_credentials") {
        AppError::bad_request("grant_type must be client_credentials").write_to(res);
        return;
    }

    match auth_client
        .issue_token(&user, &pass, "grant_type=client_credentials")
        .await
    {
        Ok(body) => res.render(Json(body)),
        Err(_) => {
            AppError::unauthorized("Invalid client credentials").write_to(res);
        }
    }
}

#[cfg(test)]
mod tests {
    use salvo::http::StatusCode;
    use salvo::test::{ResponseExt, TestClient};

    use crate::test_utils::test_service_with_auth_server;

    // -- Basic auth --

    #[tokio::test]
    async fn basic_auth_accepted() {
        let (svc, _tmp, _ts, _port) = test_service_with_auth_server().await;
        let mut resp = TestClient::get("http://localhost/service")
            .basic_auth("test", Some("password"))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["api_version"], "8.0");
    }

    #[tokio::test]
    async fn basic_auth_rejected() {
        let (svc, _tmp, _ts, _port) = test_service_with_auth_server().await;
        let mut resp = TestClient::get("http://localhost/service")
            .basic_auth("wrong", Some("creds"))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::UNAUTHORIZED);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["type"], "unauthorized");
        assert!(body["summary"].is_string());
        assert!(body["time"].is_string());
    }

    // -- Bearer token --

    #[tokio::test]
    async fn bearer_token_accepted() {
        let (svc, _tmp, ts, _port) = test_service_with_auth_server().await;
        let (token, _) = ts.issue().await;
        let mut resp = TestClient::get("http://localhost/service")
            .add_header("authorization", format!("Bearer {token}"), true)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["api_version"], "8.0");
    }

    #[tokio::test]
    async fn bearer_token_rejected() {
        let (svc, _tmp, _ts, _port) = test_service_with_auth_server().await;
        let mut resp = TestClient::get("http://localhost/service")
            .add_header("authorization", "Bearer invalid-garbage-token", true)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::UNAUTHORIZED);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["type"], "unauthorized");
        assert!(body["summary"].is_string());
        assert!(body["time"].is_string());
    }

    #[tokio::test]
    async fn bearer_token_expired() {
        let (svc, _tmp, ts, _port) = test_service_with_auth_server().await;
        let token = ts.issue_expired().await;
        let mut resp = TestClient::get("http://localhost/service")
            .add_header("authorization", format!("Bearer {token}"), true)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::UNAUTHORIZED);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["type"], "unauthorized");
        assert!(body["summary"].is_string());
        assert!(body["time"].is_string());
    }

    // -- API key --

    #[tokio::test]
    async fn api_key_accepted() {
        let (svc, _tmp, _ts, _port) = test_service_with_auth_server().await;
        let mut resp = TestClient::get("http://localhost/service?access_token=test-api-key")
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["api_version"], "8.0");
    }

    #[tokio::test]
    async fn api_key_rejected() {
        let (svc, _tmp, _ts, _port) = test_service_with_auth_server().await;
        let mut resp = TestClient::get("http://localhost/service?access_token=wrong-key")
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::UNAUTHORIZED);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["type"], "unauthorized");
        assert!(body["summary"].is_string());
        assert!(body["time"].is_string());
    }

    // -- No auth --

    #[tokio::test]
    async fn no_auth_returns_401() {
        let (svc, _tmp, _ts, _port) = test_service_with_auth_server().await;
        let mut resp = TestClient::get("http://localhost/service").send(&svc).await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::UNAUTHORIZED);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["type"], "unauthorized");
        assert!(body["summary"].is_string());
        assert!(body["time"].is_string());
    }

    // -- POST /token --

    #[tokio::test]
    async fn token_endpoint_issues_token() {
        let (svc, _tmp, _ts, _port) = test_service_with_auth_server().await;
        let mut resp = TestClient::post("http://localhost/token")
            .basic_auth("test", Some("password"))
            .raw_form("grant_type=client_credentials")
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert!(body["access_token"].is_string());
        assert_eq!(body["expires_in"], 3600);
    }

    #[tokio::test]
    async fn token_endpoint_accepts_scope() {
        let (svc, _tmp, _ts, _port) = test_service_with_auth_server().await;
        let mut resp = TestClient::post("http://localhost/token")
            .basic_auth("test", Some("password"))
            .raw_form("grant_type=client_credentials&scope=tams-api/read+tams-api/write")
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert!(body["access_token"].is_string());
    }

    #[tokio::test]
    async fn token_endpoint_rejects_wrong_creds() {
        let (svc, _tmp, _ts, _port) = test_service_with_auth_server().await;
        let mut resp = TestClient::post("http://localhost/token")
            .basic_auth("wrong", Some("creds"))
            .raw_form("grant_type=client_credentials")
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::UNAUTHORIZED);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["type"], "unauthorized");
        assert!(body["summary"].is_string());
        assert!(body["time"].is_string());
    }

    #[tokio::test]
    async fn token_endpoint_no_auth_returns_401() {
        let (svc, _tmp, _ts, _port) = test_service_with_auth_server().await;
        let mut resp = TestClient::post("http://localhost/token")
            .raw_form("grant_type=client_credentials")
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::UNAUTHORIZED);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["type"], "unauthorized");
        assert!(body["summary"].is_string());
        assert!(body["time"].is_string());
    }

    #[tokio::test]
    async fn token_endpoint_rejects_wrong_grant_type() {
        let (svc, _tmp, _ts, _port) = test_service_with_auth_server().await;
        let mut resp = TestClient::post("http://localhost/token")
            .basic_auth("test", Some("password"))
            .raw_form("grant_type=authorization_code")
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::BAD_REQUEST);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["type"], "bad_request");
        assert!(body["summary"].is_string());
        assert!(body["time"].is_string());
    }

    // -- OR semantics --

    #[tokio::test]
    async fn auth_or_semantics_both_valid() {
        let (svc, _tmp, _ts, _port) = test_service_with_auth_server().await;
        let mut resp = TestClient::get("http://localhost/service?access_token=test-api-key")
            .basic_auth("test", Some("password"))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["api_version"], "8.0");
    }

    #[tokio::test]
    async fn auth_or_semantics_invalid_header_valid_query() {
        let (svc, _tmp, _ts, _port) = test_service_with_auth_server().await;
        let mut resp = TestClient::get("http://localhost/service?access_token=test-api-key")
            .basic_auth("wrong", Some("creds"))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["api_version"], "8.0");
    }

    #[tokio::test]
    async fn auth_or_semantics_valid_header_invalid_query() {
        let (svc, _tmp, _ts, _port) = test_service_with_auth_server().await;
        let mut resp = TestClient::get("http://localhost/service?access_token=wrong-key")
            .basic_auth("test", Some("password"))
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["api_version"], "8.0");
    }

    // -- Malformed auth headers --

    #[tokio::test]
    async fn malformed_basic_no_credentials() {
        let (svc, _tmp, _ts, _port) = test_service_with_auth_server().await;
        let mut resp = TestClient::get("http://localhost/service")
            .add_header("authorization", "Basic", true)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::UNAUTHORIZED);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["type"], "unauthorized");
        assert!(body["summary"].is_string());
        assert!(body["time"].is_string());
    }

    #[tokio::test]
    async fn malformed_basic_not_base64() {
        let (svc, _tmp, _ts, _port) = test_service_with_auth_server().await;
        let mut resp = TestClient::get("http://localhost/service")
            .add_header("authorization", "Basic not-valid-base64!!!", true)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::UNAUTHORIZED);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["type"], "unauthorized");
        assert!(body["summary"].is_string());
        assert!(body["time"].is_string());
    }

    #[tokio::test]
    async fn malformed_bearer_empty_token() {
        let (svc, _tmp, _ts, _port) = test_service_with_auth_server().await;
        let mut resp = TestClient::get("http://localhost/service")
            .add_header("authorization", "Bearer ", true)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::UNAUTHORIZED);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["type"], "unauthorized");
        assert!(body["summary"].is_string());
        assert!(body["time"].is_string());
    }

    #[tokio::test]
    async fn empty_access_token_query_param() {
        let (svc, _tmp, _ts, _port) = test_service_with_auth_server().await;
        let mut resp = TestClient::get("http://localhost/service?access_token=")
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::UNAUTHORIZED);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["type"], "unauthorized");
        assert!(body["summary"].is_string());
        assert!(body["time"].is_string());
    }

    // -- Integration: token -> API call --

    #[tokio::test]
    async fn issued_token_works_for_api_calls() {
        let (svc, _tmp, _ts, _port) = test_service_with_auth_server().await;
        // Get a token
        let mut resp = TestClient::post("http://localhost/token")
            .basic_auth("test", Some("password"))
            .raw_form("grant_type=client_credentials")
            .send(&svc)
            .await;
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let token = body["access_token"].as_str().unwrap();

        // Use it
        let mut resp = TestClient::get("http://localhost/service")
            .add_header("authorization", format!("Bearer {token}"), true)
            .send(&svc)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["api_version"], "8.0");
    }
}

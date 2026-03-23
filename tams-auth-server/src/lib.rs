//! tams-auth-server library -- builds the auth service router.
//!
//! Separated from the binary so other crates (e.g. tams-server tests)
//! can embed the auth service.

use salvo::http::StatusCode;
use salvo::prelude::*;

use tams_auth::{
    authenticate, check_basic_credentials, decode_basic_auth, AuthRequest, TokenStore,
};

/// POST /auth/check
///
/// Validate credentials. Body is an AuthRequest JSON.
/// Returns 200 if valid, 401 if invalid.
#[handler]
async fn check_auth(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let auth_req: AuthRequest = match req.parse_json().await {
        Ok(r) => r,
        Err(_) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({
                "error": "Invalid AuthRequest JSON body"
            })));
            return;
        }
    };

    let token_store = depot
        .obtain::<TokenStore>()
        .expect("TokenStore not in depot");
    match authenticate(&auth_req, token_store).await {
        Ok(()) => {
            res.status_code(StatusCode::OK);
            res.render(Json(serde_json::json!({"status": "ok"})));
        }
        Err(_) => {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(Json(serde_json::json!({"error": "Invalid credentials"})));
        }
    }
}

/// POST /auth/token
///
/// Issue a bearer token. Expects Basic auth header + form body with
/// `grant_type=client_credentials`.
/// Returns `{"access_token": "...", "token_type": "bearer", "expires_in": 3600}`.
#[handler]
async fn issue_token(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    // Validate Basic auth
    let authed = req
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Basic "))
        .and_then(decode_basic_auth)
        .is_some_and(|(user, pass)| check_basic_credentials(&user, &pass));

    if !authed {
        res.status_code(StatusCode::UNAUTHORIZED);
        res.render(Json(
            serde_json::json!({"error": "Invalid client credentials"}),
        ));
        return;
    }

    // Validate grant_type
    let form = req.form_data().await;
    let grant_type = form.ok().and_then(|f| f.fields.get("grant_type").cloned());
    if grant_type.as_deref() != Some("client_credentials") {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(
            serde_json::json!({"error": "grant_type must be client_credentials"}),
        ));
        return;
    }

    let token_store = depot
        .obtain::<TokenStore>()
        .expect("TokenStore not in depot");
    let (access_token, expires_in) = token_store.issue().await;
    res.render(Json(serde_json::json!({
        "access_token": access_token,
        "token_type": "bearer",
        "expires_in": expires_in
    })));
}

pub fn build_router(token_store: TokenStore) -> Router {
    Router::new()
        .hoop(salvo::affix_state::inject(token_store))
        .push(Router::with_path("auth/check").post(check_auth))
        .push(Router::with_path("auth/token").post(issue_token))
}

pub fn build_service(token_store: TokenStore) -> Service {
    Service::new(build_router(token_store))
}

#[cfg(test)]
mod tests {
    use salvo::http::StatusCode;
    use salvo::test::{ResponseExt, TestClient};

    use tams_auth::TokenStore;

    use super::build_service;

    #[tokio::test]
    async fn check_basic_valid() {
        let service = build_service(TokenStore::new());
        let mut resp = TestClient::post("http://localhost/auth/check")
            .json(&serde_json::json!({"auth_type": "Basic", "user": "test", "pass": "password"}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn check_basic_invalid() {
        let service = build_service(TokenStore::new());
        let resp = TestClient::post("http://localhost/auth/check")
            .json(&serde_json::json!({"auth_type": "Basic", "user": "wrong", "pass": "creds"}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn check_api_key_valid() {
        let service = build_service(TokenStore::new());
        let resp = TestClient::post("http://localhost/auth/check")
            .json(&serde_json::json!({"auth_type": "ApiKey", "key": "test-api-key"}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
    }

    #[tokio::test]
    async fn check_api_key_invalid() {
        let service = build_service(TokenStore::new());
        let resp = TestClient::post("http://localhost/auth/check")
            .json(&serde_json::json!({"auth_type": "ApiKey", "key": "wrong"}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn check_bearer_valid() {
        let ts = TokenStore::new();
        let (token, _) = ts.issue().await;
        let service = build_service(ts);
        let resp = TestClient::post("http://localhost/auth/check")
            .json(&serde_json::json!({"auth_type": "Bearer", "token": token}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
    }

    #[tokio::test]
    async fn check_bearer_invalid() {
        let service = build_service(TokenStore::new());
        let resp = TestClient::post("http://localhost/auth/check")
            .json(&serde_json::json!({"auth_type": "Bearer", "token": "garbage"}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn check_bearer_expired() {
        let ts = TokenStore::new();
        let token = ts.issue_expired().await;
        let service = build_service(ts);
        let resp = TestClient::post("http://localhost/auth/check")
            .json(&serde_json::json!({"auth_type": "Bearer", "token": token}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn check_bad_json() {
        let service = build_service(TokenStore::new());
        let resp = TestClient::post("http://localhost/auth/check")
            .json(&serde_json::json!({"nonsense": true}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn token_issue_valid() {
        let service = build_service(TokenStore::new());
        let mut resp = TestClient::post("http://localhost/auth/token")
            .basic_auth("test", Some("password"))
            .raw_form("grant_type=client_credentials")
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: serde_json::Value = resp.take_json().await.unwrap();
        assert!(body["access_token"].is_string());
        assert_eq!(body["token_type"], "bearer");
        assert_eq!(body["expires_in"], 3600);
    }

    #[tokio::test]
    async fn token_issue_wrong_creds() {
        let service = build_service(TokenStore::new());
        let resp = TestClient::post("http://localhost/auth/token")
            .basic_auth("wrong", Some("creds"))
            .raw_form("grant_type=client_credentials")
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn token_issue_wrong_grant_type() {
        let service = build_service(TokenStore::new());
        let resp = TestClient::post("http://localhost/auth/token")
            .basic_auth("test", Some("password"))
            .raw_form("grant_type=authorization_code")
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn token_issue_no_auth() {
        let service = build_service(TokenStore::new());
        let resp = TestClient::post("http://localhost/auth/token")
            .raw_form("grant_type=client_credentials")
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn issued_token_validates() {
        let ts = TokenStore::new();
        let service = build_service(ts.clone());

        // Issue token
        let mut resp = TestClient::post("http://localhost/auth/token")
            .basic_auth("test", Some("password"))
            .raw_form("grant_type=client_credentials")
            .send(&service)
            .await;
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let token = body["access_token"].as_str().unwrap();

        // Validate it
        let resp = TestClient::post("http://localhost/auth/check")
            .json(&serde_json::json!({"auth_type": "Bearer", "token": token}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
    }
}

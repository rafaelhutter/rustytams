/// HTTP client for the tams-auth-server.
///
/// The TAMS server delegates all authentication to an external auth service
/// via HTTP, making the auth backend swappable.

#[derive(Clone)]
pub struct AuthClient {
    client: reqwest::Client,
    auth_url: String,
}

impl AuthClient {
    pub fn new(auth_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            auth_url: auth_url.trim_end_matches('/').to_string(),
        }
    }

    /// POST /auth/check — validate credentials.
    /// Returns true if the auth-server responds 200 (valid).
    pub async fn check(&self, auth_request: &serde_json::Value) -> bool {
        let resp = self
            .client
            .post(format!("{}/auth/check", self.auth_url))
            .json(auth_request)
            .send()
            .await;
        matches!(resp, Ok(r) if r.status().is_success())
    }

    /// POST /auth/token — issue a bearer token.
    /// Forwards Basic auth credentials and form body to the auth-server.
    /// Returns Ok(body) only if the auth-server responds with 2xx.
    pub async fn issue_token(
        &self,
        user: &str,
        pass: &str,
        form_body: &str,
    ) -> Result<serde_json::Value, String> {
        let resp = self
            .client
            .post(format!("{}/auth/token", self.auth_url))
            .basic_auth(user, Some(pass))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(form_body.to_string())
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            return Err(format!("Auth server returned {}", resp.status()));
        }

        resp.json().await.map_err(|e| e.to_string())
    }
}

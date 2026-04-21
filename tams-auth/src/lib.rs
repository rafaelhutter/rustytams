use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use uuid::Uuid;

// Kept for backwards compatibility in tests
pub const BASIC_USER: &str = "test";
pub const BASIC_PASS: &str = "password";
pub const API_KEY: &str = "test-api-key";

/// Runtime-configurable credentials store.
#[derive(Clone, Debug)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

impl Default for Credentials {
    fn default() -> Self {
        Self {
            username: BASIC_USER.to_string(),
            password: BASIC_PASS.to_string(),
        }
    }
}

impl Credentials {
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self { username: username.into(), password: password.into() }
    }

    pub fn check(&self, user: &str, pass: &str) -> bool {
        self.username == user && self.password == pass
    }
}

// -- Token store --

struct TokenEntry {
    expires_at: Instant,
}

#[derive(Clone)]
pub struct TokenStore {
    tokens: Arc<RwLock<HashMap<String, TokenEntry>>>,
    ttl: Duration,
}

impl Default for TokenStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenStore {
    pub fn new() -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
            ttl: Duration::from_secs(3600),
        }
    }

    pub async fn issue(&self) -> (String, u64) {
        let token = Uuid::new_v4().to_string();
        let entry = TokenEntry {
            expires_at: Instant::now() + self.ttl,
        };
        let mut tokens = self.tokens.write().await;
        // Opportunistic cleanup of expired tokens to prevent unbounded growth
        let now = Instant::now();
        tokens.retain(|_, e| e.expires_at > now);
        tokens.insert(token.clone(), entry);
        (token, self.ttl.as_secs())
    }

    pub async fn validate(&self, token: &str) -> bool {
        let tokens = self.tokens.read().await;
        match tokens.get(token) {
            Some(entry) => entry.expires_at > Instant::now(),
            None => false,
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn issue_expired(&self) -> String {
        let token = Uuid::new_v4().to_string();
        let entry = TokenEntry {
            expires_at: Instant::now() - Duration::from_secs(1),
        };
        self.tokens.write().await.insert(token.clone(), entry);
        token
    }
}

// -- Authentication logic --

/// Parsed authentication request.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "auth_type")]
pub enum AuthRequest {
    Basic { user: String, pass: String },
    Bearer { token: String },
    ApiKey { key: String },
}

/// Authentication error.
pub enum AuthError {
    InvalidCredentials,
}

/// Validate an authentication request against the token store.
pub async fn authenticate(req: &AuthRequest, token_store: &TokenStore) -> Result<(), AuthError> {
    match req {
        AuthRequest::Basic { user, pass } => {
            if check_basic_credentials(user, pass) {
                Ok(())
            } else {
                Err(AuthError::InvalidCredentials)
            }
        }
        AuthRequest::Bearer { token } => {
            if token_store.validate(token).await {
                Ok(())
            } else {
                Err(AuthError::InvalidCredentials)
            }
        }
        AuthRequest::ApiKey { key } => {
            if check_api_key(key) {
                Ok(())
            } else {
                Err(AuthError::InvalidCredentials)
            }
        }
    }
}

/// Check if basic credentials are valid against the hardcoded test defaults.
/// Use `Credentials::check` for runtime-configurable credentials.
pub fn check_basic_credentials(user: &str, pass: &str) -> bool {
    user == BASIC_USER && pass == BASIC_PASS
}

/// Check if an API key is valid.
pub fn check_api_key(key: &str) -> bool {
    key == API_KEY
}

/// Decode a base64-encoded Basic auth string into (user, pass).
pub fn decode_basic_auth(encoded: &str) -> Option<(String, String)> {
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let creds = String::from_utf8(decoded).ok()?;
    let (user, pass) = creds.split_once(':')?;
    Some((user.to_string(), pass.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_credentials_valid() {
        assert!(check_basic_credentials("test", "password"));
    }

    #[test]
    fn basic_credentials_invalid() {
        assert!(!check_basic_credentials("wrong", "creds"));
    }

    #[test]
    fn api_key_valid() {
        assert!(check_api_key("test-api-key"));
    }

    #[test]
    fn api_key_invalid() {
        assert!(!check_api_key("wrong-key"));
    }

    #[test]
    fn decode_basic_auth_valid() {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode("test:password");
        let (user, pass) = decode_basic_auth(&encoded).unwrap();
        assert_eq!(user, "test");
        assert_eq!(pass, "password");
    }

    #[test]
    fn decode_basic_auth_invalid() {
        assert!(decode_basic_auth("not-valid-base64!!!").is_none());
    }

    #[tokio::test]
    async fn token_store_issue_and_validate() {
        let store = TokenStore::new();
        let (token, expires_in) = store.issue().await;
        assert_eq!(expires_in, 3600);
        assert!(store.validate(&token).await);
    }

    #[tokio::test]
    async fn token_store_invalid_token() {
        let store = TokenStore::new();
        assert!(!store.validate("nonexistent").await);
    }

    #[tokio::test]
    async fn token_store_expired_token() {
        let store = TokenStore::new();
        let token = store.issue_expired().await;
        assert!(!store.validate(&token).await);
    }

    #[tokio::test]
    async fn authenticate_basic_valid() {
        let store = TokenStore::new();
        let req = AuthRequest::Basic {
            user: "test".into(),
            pass: "password".into(),
        };
        assert!(authenticate(&req, &store).await.is_ok());
    }

    #[tokio::test]
    async fn authenticate_basic_invalid() {
        let store = TokenStore::new();
        let req = AuthRequest::Basic {
            user: "wrong".into(),
            pass: "creds".into(),
        };
        assert!(authenticate(&req, &store).await.is_err());
    }

    #[tokio::test]
    async fn authenticate_bearer_valid() {
        let store = TokenStore::new();
        let (token, _) = store.issue().await;
        let req = AuthRequest::Bearer { token };
        assert!(authenticate(&req, &store).await.is_ok());
    }

    #[tokio::test]
    async fn authenticate_bearer_invalid() {
        let store = TokenStore::new();
        let req = AuthRequest::Bearer {
            token: "nonexistent-token".into(),
        };
        assert!(authenticate(&req, &store).await.is_err());
    }

    #[tokio::test]
    async fn authenticate_bearer_expired() {
        let store = TokenStore::new();
        let token = store.issue_expired().await;
        let req = AuthRequest::Bearer { token };
        assert!(authenticate(&req, &store).await.is_err());
    }

    #[tokio::test]
    async fn authenticate_api_key_valid() {
        let store = TokenStore::new();
        let req = AuthRequest::ApiKey {
            key: "test-api-key".into(),
        };
        assert!(authenticate(&req, &store).await.is_ok());
    }

    #[tokio::test]
    async fn authenticate_api_key_invalid() {
        let store = TokenStore::new();
        let req = AuthRequest::ApiKey {
            key: "wrong-key".into(),
        };
        assert!(authenticate(&req, &store).await.is_err());
    }
}

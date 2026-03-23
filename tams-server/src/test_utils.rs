#[cfg(test)]
use salvo::prelude::Service;
#[cfg(test)]
use tams_auth::TokenStore;
#[cfg(test)]
use tams_store::Store;
#[cfg(test)]
use tempfile::TempDir;

use crate::auth_client::AuthClient;

/// Create a test service with an embedded auth server on a random port.
///
/// Returns (service, store, tmp_dir, token_store, auth_port).
#[cfg(test)]
pub async fn test_service_with_auth_server() -> (Service, TempDir, TokenStore, u16) {
    let tmp = TempDir::new().unwrap();
    let store = Store::new_test(tmp.path()).await.unwrap();
    let token_store = TokenStore::new();

    // Start an embedded auth server on a random port
    let auth_port = start_auth_server(token_store.clone()).await;
    let auth_client = AuthClient::new(&format!("http://127.0.0.1:{auth_port}"));

    let service = crate::router::build_service(store, auth_client);
    (service, tmp, token_store, auth_port)
}

/// Convenience wrapper for tests that don't need the TokenStore.
#[cfg(test)]
pub async fn test_service() -> (Service, Store, TempDir) {
    let tmp = TempDir::new().unwrap();
    let store = Store::new_test(tmp.path()).await.unwrap();
    let token_store = TokenStore::new();

    let auth_port = start_auth_server(token_store).await;
    let auth_client = AuthClient::new(&format!("http://127.0.0.1:{auth_port}"));

    let service = crate::router::build_service(store.clone(), auth_client);
    (service, store, tmp)
}

/// Start the auth-server on a random port. Returns the port number.
#[cfg(test)]
async fn start_auth_server(token_store: TokenStore) -> u16 {
    use salvo::Listener;

    // Find a free port by binding to port 0
    let free_port = {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap().port()
    };

    let service = tams_auth_server::build_service(token_store);
    let acceptor = salvo::prelude::TcpListener::new(format!("127.0.0.1:{free_port}"))
        .bind()
        .await;
    tokio::spawn(salvo::prelude::Server::new(acceptor).serve(service));

    // Give the server a moment to start accepting connections
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    free_port
}

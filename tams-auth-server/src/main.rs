//! tams-auth-server -- Authentication service for TAMS.
//!
//! Validates credentials and issues bearer tokens.
//! Called by tams-server to authenticate incoming requests.

use clap::Parser;
use salvo::prelude::*;
use salvo::Listener;

use tams_auth::{Credentials, TokenStore};

#[derive(Parser)]
struct Config {
    /// Port to listen on.
    #[arg(long, default_value = "5802")]
    port: u16,

    /// Username for Basic auth (overrides TAMS_USERNAME env var).
    #[arg(long, env = "TAMS_USERNAME", default_value = "test")]
    username: String,

    /// Password for Basic auth (overrides TAMS_PASSWORD env var).
    #[arg(long, env = "TAMS_PASSWORD", default_value = "password")]
    password: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let config = Config::parse();

    let credentials = Credentials::new(&config.username, &config.password);
    tracing::info!("tams-auth-server: using username '{}'", config.username);

    let token_store = TokenStore::new();
    let service = tams_auth_server::build_service(token_store, credentials);
    let acceptor = TcpListener::new(format!("0.0.0.0:{}", config.port))
        .bind()
        .await;
    tracing::info!("tams-auth-server listening on port {}", config.port);
    Server::new(acceptor).serve(service).await;
}

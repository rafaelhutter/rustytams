//! tams-auth-server -- Authentication service for TAMS.
//!
//! Validates credentials and issues bearer tokens.
//! Called by tams-server to authenticate incoming requests.

use clap::Parser;
use salvo::prelude::*;
use salvo::Listener;

use tams_auth::TokenStore;

#[derive(Parser)]
struct Config {
    /// Port to listen on.
    #[arg(long, default_value = "5802")]
    port: u16,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let config = Config::parse();

    let token_store = TokenStore::new();
    let service = tams_auth_server::build_service(token_store);
    let acceptor = TcpListener::new(format!("0.0.0.0:{}", config.port))
        .bind()
        .await;
    tracing::info!("tams-auth-server listening on port {}", config.port);
    Server::new(acceptor).serve(service).await;
}

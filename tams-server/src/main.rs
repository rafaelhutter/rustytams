//! RustyTAMS -- a fully spec-conforming TAMS server in Rust.

mod auth_client;
mod auth_middleware;
mod config;
mod error;
mod extract;
mod handlers;
mod router;
#[cfg(test)]
mod test_utils;

use auth_client::AuthClient;
use clap::Parser;
use config::Config;
use salvo::Listener;
use tams_store::{S3Config, Store};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let config = Config::parse();

    let s3_config = S3Config {
        endpoint: config.s3_endpoint.clone(),
        bucket: config.s3_bucket.clone(),
        access_key: config.s3_access_key.clone(),
        secret_key: config.s3_secret_key.clone(),
        region: config.s3_region.clone(),
    };

    let store = Store::new(&config.data_dir, s3_config)
        .await
        .expect("failed to initialize store");
    let auth_client = AuthClient::new(&config.auth_url);

    let service = router::build_service(store, auth_client);
    let acceptor = salvo::prelude::TcpListener::new(format!("0.0.0.0:{}", config.port))
        .bind()
        .await;
    tracing::info!("RustyTAMS listening on port {}", config.port);
    salvo::prelude::Server::new(acceptor).serve(service).await;
}

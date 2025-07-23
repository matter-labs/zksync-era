use axum::extract::DefaultBodyLimit;
use axum::{routing::{get, post}, Router};
use proof_cache::handlers::{fri_handlers, linked_handlers};
use proof_cache::state::AppState;
use proof_cache::storage::MemoryStorage;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::Level;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let storage = AppState::new(Arc::new(MemoryStorage::new()));

    let app = Router::new()
        .route("/fri_proofs/{key}", get(fri_handlers::get))
        .route("/fri_proofs/", post(fri_handlers::put))
        .route("/linked_proofs/", get(linked_handlers::get))
        .route("/linked_proofs/", post(linked_handlers::put))
        // 10 MBs
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .with_state(storage);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3815")
        .await
        .expect("Failed to bind TCP listener");

    tracing::info!("Starting proof cache server at {}", listener.local_addr().expect("failed to get address"));

    axum::serve(listener, app)
        .await
        .expect("Failed to start server");
}

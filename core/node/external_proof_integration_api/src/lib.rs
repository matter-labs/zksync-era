mod error;
mod metrics;
mod processor;

use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use axum::{
    extract::{Multipart, Path},
    middleware,
    routing::post,
    Json, Router,
};
use tokio::sync::watch;
use zksync_basic_types::commitment::L1BatchCommitmentMode;
use zksync_config::configs::external_proof_integration_api::ExternalProofIntegrationApiConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_prover_interface::api::OptionalProofGenerationDataRequest;

use crate::processor::Processor;

pub async fn run_server(
    config: ExternalProofIntegrationApiConfig,
    blob_store: Arc<dyn ObjectStore>,
    connection_pool: ConnectionPool<Core>,
    commitment_mode: L1BatchCommitmentMode,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let bind_address = SocketAddr::from(([0, 0, 0, 0], config.http_port));
    tracing::debug!("Starting external prover API server on {bind_address}");
    let app = create_router(blob_store, connection_pool, commitment_mode).await;

    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .with_context(|| format!("Failed binding external prover API server to {bind_address}"))?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            if stop_receiver.changed().await.is_err() {
                tracing::warn!("Stop signal sender for external prover API server was dropped without sending a signal");
            }
            tracing::info!("Stop signal received, external prover API server is shutting down");
        })
        .await
        .context("External prover API server failed")?;
    tracing::info!("External prover API server shut down");
    Ok(())
}

async fn create_router(
    blob_store: Arc<dyn ObjectStore>,
    connection_pool: ConnectionPool<Core>,
    commitment_mode: L1BatchCommitmentMode,
) -> Router {
    let mut processor =
        Processor::new(blob_store.clone(), connection_pool.clone(), commitment_mode);
    let verify_proof_processor = processor.clone();
    Router::new()
        .route(
            "/proof_generation_data",
            post(
                // we use post method because the returned data is not idempotent,
                // i.e we return different result on each call.
                move |payload: Json<OptionalProofGenerationDataRequest>| async move {
                    processor.get_proof_generation_data(payload).await
                },
            ),
        )
        .route(
            "/verify_proof/:l1_batch_number",
            post(
                move |l1_batch_number: Path<u32>, multipart: Multipart| async move {
                    verify_proof_processor
                        .verify_proof(l1_batch_number, multipart)
                        .await
                },
            ),
        )
        .layer(middleware::from_fn(metrics::call_outcome_tracker))
}

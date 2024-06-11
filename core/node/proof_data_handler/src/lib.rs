use std::{net::SocketAddr, sync::Arc};

use anyhow::Context as _;
use axum::{extract::Path, routing::post, Json, Router};
use tokio::sync::watch;
use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_prover_interface::api::{
    ProofGenerationDataRequest, SubmitProofRequest, SubmitTeeProofRequest,
    TeeProofGenerationDataRequest,
};
use zksync_types::commitment::L1BatchCommitmentMode;

use crate::request_processor::RequestProcessor;
use crate::tee_request_processor::TeeRequestProcessor;

#[cfg(test)]
mod tests;

mod errors;
mod request_processor;
mod tee_request_processor;

pub async fn run_server(
    config: ProofDataHandlerConfig,
    blob_store: Arc<dyn ObjectStore>,
    connection_pool: ConnectionPool<Core>,
    commitment_mode: L1BatchCommitmentMode,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let bind_address = SocketAddr::from(([0, 0, 0, 0], config.http_port));
    tracing::debug!("Starting proof data handler server on {bind_address}");
    let app = create_proof_processing_router(blob_store, connection_pool, config, commitment_mode);

    axum::Server::bind(&bind_address)
        .serve(app.into_make_service())
        .with_graceful_shutdown(async move {
            if stop_receiver.changed().await.is_err() {
                tracing::warn!("Stop signal sender for proof data handler server was dropped without sending a signal");
            }
            tracing::info!("Stop signal received, proof data handler server is shutting down");
        })
        .await
        .context("Proof data handler server failed")?;
    tracing::info!("Proof data handler server shut down");
    Ok(())
}

fn create_proof_processing_router(
    blob_store: Arc<dyn ObjectStore>,
    connection_pool: ConnectionPool<Core>,
    config: ProofDataHandlerConfig,
    commitment_mode: L1BatchCommitmentMode,
) -> Router {
    let get_tee_proof_gen_processor =
        TeeRequestProcessor::new(blob_store.clone(), connection_pool.clone(), config.clone());
    let submit_tee_proof_processor = get_tee_proof_gen_processor.clone();
    let get_proof_gen_processor =
        RequestProcessor::new(blob_store, connection_pool, config, commitment_mode);
    let submit_proof_processor = get_proof_gen_processor.clone();

    Router::new()
        .route(
            "/proof_generation_data",
            post(
                // we use post method because the returned data is not idempotent,
                // i.e we return different result on each call.
                move |payload: Json<ProofGenerationDataRequest>| async move {
                    get_proof_gen_processor
                        .get_proof_generation_data(payload)
                        .await
                },
            ),
        )
        .route(
            "/submit_proof/:l1_batch_number",
            post(
                move |l1_batch_number: Path<u32>, payload: Json<SubmitProofRequest>| async move {
                    submit_proof_processor
                        .submit_proof(l1_batch_number, payload)
                        .await
                },
            ),
        )
        .route(
            "/tee_proof_generation_data",
            post(
                move |payload: Json<TeeProofGenerationDataRequest>| async move {
                    get_tee_proof_gen_processor
                        .get_proof_generation_data(payload)
                        .await
                },
            ),
        )
        .route(
            "/submit_tee_proof/:l1_batch_number",
            post(
                move |l1_batch_number: Path<u32>, payload: Json<SubmitTeeProofRequest>| async move {
                    submit_tee_proof_processor
                        .submit_proof(l1_batch_number, payload)
                        .await
                },
            ),
        )
}

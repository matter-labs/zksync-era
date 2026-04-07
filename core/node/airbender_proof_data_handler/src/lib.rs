use std::{net::SocketAddr, sync::Arc};

use airbender_request_processor::AirbenderRequestProcessor;
use anyhow::Context as _;
use axum::{
    extract::Path,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use tokio::sync::watch;
use zksync_airbender_prover_interface::api::{
    AirbenderProofGenerationDataRequest, RegisterAirbenderAttestationRequest,
    SubmitAirbenderProofRequest,
};
use zksync_config::configs::AirbenderProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::{commitment::L1BatchCommitmentMode, L2ChainId};

mod airbender_request_processor;
mod errors;
mod metrics;
pub mod node;
#[cfg(test)]
mod tests;

pub async fn run_server(
    config: AirbenderProofDataHandlerConfig,
    blob_store: Arc<dyn ObjectStore>,
    connection_pool: ConnectionPool<Core>,
    commitment_mode: L1BatchCommitmentMode,
    l2_chain_id: L2ChainId,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let bind_address = SocketAddr::from(([0, 0, 0, 0], config.http_port));
    tracing::info!("Starting proof data handler server on {bind_address}");
    let app = create_proof_processing_router(
        blob_store,
        connection_pool,
        config,
        commitment_mode,
        l2_chain_id,
    );

    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .with_context(|| format!("Failed binding proof data handler server to {bind_address}"))?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            if stop_receiver.changed().await.is_err() {
                tracing::warn!("Stop request sender for proof data handler server was dropped without sending a signal");
            }
            tracing::info!("Stop request received, proof data handler server is shutting down");
        })
        .await
        .context("Proof data handler server failed")?;
    tracing::info!("Proof data handler server shut down");
    Ok(())
}

fn create_proof_processing_router(
    blob_store: Arc<dyn ObjectStore>,
    connection_pool: ConnectionPool<Core>,
    config: AirbenderProofDataHandlerConfig,
    _commitment_mode: L1BatchCommitmentMode,
    l2_chain_id: L2ChainId,
) -> Router {
    let get_airbender_proof_gen_processor =
        AirbenderRequestProcessor::new(blob_store, connection_pool, config.clone(), l2_chain_id);
    let submit_airbender_proof_processor = get_airbender_proof_gen_processor.clone();
    let register_tee_attestation_processor = get_airbender_proof_gen_processor.clone();
    let observe_airbender_proof_gen_processor = get_airbender_proof_gen_processor.clone();
    let present_batches_processor = get_airbender_proof_gen_processor.clone();

    let router =
        Router::new()
            .route(
                "/airbender/proof_inputs",
                post(
                    move |payload: Json<AirbenderProofGenerationDataRequest>| async move {
                        let result = get_airbender_proof_gen_processor
                            .get_proof_generation_data(payload)
                            .await;

                        match result {
                            Ok(Some(data)) => (StatusCode::OK, data).into_response(),
                            Ok(None) => StatusCode::NO_CONTENT.into_response(),
                            Err(e) => e.into_response(),
                        }
                    },
                ),
            )
            .route(
                "/airbender/proof_inputs_no_lock/{batch}",
                get(move |batch: Path<u32>| async move {
                    let result = observe_airbender_proof_gen_processor
                        .get_proof_generation_data_no_lock(batch)
                        .await;

                    match result {
                        Ok(Some(data)) => (StatusCode::OK, data).into_response(),
                        Ok(None) => StatusCode::NOT_FOUND.into_response(),
                        Err(e) => e.into_response(),
                    }
                }),
            )
            .route(
                "/airbender/present_batches",
                get(move || async move {
                    let result = present_batches_processor.get_present_batches().await;

                    match result {
                        Ok(data) => (StatusCode::OK, data).into_response(),
                        Err(e) => e.into_response(),
                    }
                }),
            )
            .route(
                "/airbender/submit_proofs/{l1_batch_number}",
                post(
                    move |l1_batch_number: Path<u32>,
                          payload: Json<SubmitAirbenderProofRequest>| async move {
                        submit_airbender_proof_processor
                            .submit_proof(l1_batch_number, payload)
                            .await
                    },
                ),
            )
            .route(
                "/airbender/register_attestation",
                post(
                    move |payload: Json<RegisterAirbenderAttestationRequest>| async move {
                        register_tee_attestation_processor
                            .register_tee_attestation(payload)
                            .await
                    },
                ),
            );

    router
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(tower_http::decompression::RequestDecompressionLayer::new().zstd(true))
}

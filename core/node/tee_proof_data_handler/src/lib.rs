use std::{future::IntoFuture, net::SocketAddr, sync::Arc};

use anyhow::Context as _;
use axum::{extract::Path, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use tee_request_processor::TeeRequestProcessor;
use tokio::sync::watch;
use zksync_config::configs::TeeProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_tee_prover_interface::api::{
    RegisterTeeAttestationRequest, SubmitTeeProofRequest, TeeProofGenerationDataRequest,
};
use zksync_types::{commitment::L1BatchCommitmentMode, L2ChainId};

mod collateral;
mod errors;
mod metrics;
pub mod node;
mod tee_request_processor;

pub mod tee_contract;
#[cfg(test)]
mod tests;

pub async fn run_server(
    config: TeeProofDataHandlerConfig,
    blob_store: Arc<dyn ObjectStore>,
    connection_pool: ConnectionPool<Core>,
    commitment_mode: L1BatchCommitmentMode,
    l2_chain_id: L2ChainId,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let bind_address = SocketAddr::from(([0, 0, 0, 0], config.http_port));
    tracing::info!("Starting proof data handler server on {bind_address}");
    let app = create_proof_processing_router(
        blob_store.clone(),
        connection_pool.clone(),
        config.clone(),
        commitment_mode,
        l2_chain_id,
    );

    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .with_context(|| format!("Failed binding proof data handler server to {bind_address}"))?;

    let mut server_stop_receiver = stop_receiver.clone();
    let server = tokio::spawn(
            axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                if server_stop_receiver.changed().await.is_err() {
                    tracing::warn!("Stop signal sender for proof data handler server was dropped without sending a signal");
                }
                tracing::info!("Stop signal received, proof data handler server is shutting down");
            }).into_future()
        );

    let updater = tokio::spawn(collateral::updater(
        blob_store,
        connection_pool,
        config,
        commitment_mode,
        l2_chain_id,
        stop_receiver,
    ));

    // FIXME: TEE - the updater should most likely be its own node/server
    tokio::select! {
        server_result = server => {
            tracing::info!("Proof data handler server completed first");
            server_result
                .context("Proof data handler server join failed")?
                .context("Proof data handler server failed")?;
        }
        updater_result = updater => {
            tracing::info!("DCAP collateral updater completed first");
            updater_result
                .context("DCAP collateral updater join failed")?
                .context("DCAP collateral updater failed")?;
        }
    };

    tracing::info!("Proof data handler server or DCAP collateral updater shut down");
    Ok(())
}

fn create_proof_processing_router(
    blob_store: Arc<dyn ObjectStore>,
    connection_pool: ConnectionPool<Core>,
    config: TeeProofDataHandlerConfig,
    _commitment_mode: L1BatchCommitmentMode,
    l2_chain_id: L2ChainId,
) -> Router {
    let get_tee_proof_gen_processor =
        TeeRequestProcessor::new(blob_store, connection_pool, config.clone(), l2_chain_id);
    let submit_tee_proof_processor = get_tee_proof_gen_processor.clone();
    let register_tee_attestation_processor = get_tee_proof_gen_processor.clone();

    let router = Router::new()
        .route(
            "/tee/proof_inputs",
            post(
                move |payload: Json<TeeProofGenerationDataRequest>| async move {
                    let result = get_tee_proof_gen_processor
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
            "/tee/submit_proofs/:l1_batch_number",
            post(
                move |l1_batch_number: Path<u32>, payload: Json<SubmitTeeProofRequest>| async move {
                    submit_tee_proof_processor
                        .submit_proof(l1_batch_number, payload)
                        .await
                },
            ),
        )
        .route(
            "/tee/register_attestation",
            post(
                move |payload: Json<RegisterTeeAttestationRequest>| async move {
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

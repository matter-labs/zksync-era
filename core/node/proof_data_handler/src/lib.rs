use std::{net::SocketAddr, sync::Arc};

use anyhow::Context as _;
use axum::{extract::Path, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use processor::Locking;
use tee_request_processor::TeeRequestProcessor;
use tokio::sync::watch;
use zksync_config::configs::{fri_prover_gateway::ApiMode, ProofDataHandlerConfig};
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_prover_interface::api::{
    ProofGenerationDataRequest, ProofGenerationDataResponse, RegisterTeeAttestationRequest,
    SubmitProofRequest, SubmitProofResponse, SubmitTeeProofRequest, TeeProofGenerationDataRequest,
};
use zksync_types::{commitment::L1BatchCommitmentMode, L1BatchNumber, L2ChainId};

pub use crate::{
    client::ProofDataHandlerClient,
    errors::ProcessorError,
    processor::{Processor, Readonly},
};

#[cfg(test)]
mod tests;

mod client;
mod errors;
mod metrics;
mod processor;
mod tee_request_processor;

pub async fn run_server(
    config: ProofDataHandlerConfig,
    blob_store: Arc<dyn ObjectStore>,
    connection_pool: ConnectionPool<Core>,
    commitment_mode: L1BatchCommitmentMode,
    l2_chain_id: L2ChainId,
    api_mode: ApiMode,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let bind_address = SocketAddr::from(([0, 0, 0, 0], config.http_port));
    tracing::info!("Starting proof data handler server on {bind_address}");
    let app = create_proof_processing_router(
        blob_store,
        connection_pool,
        config,
        api_mode,
        commitment_mode,
        l2_chain_id,
    );

    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .with_context(|| format!("Failed binding proof data handler server to {bind_address}"))?;
    axum::serve(listener, app)
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
    api_mode: ApiMode,
    commitment_mode: L1BatchCommitmentMode,
    l2_chain_id: L2ChainId,
) -> Router {
    let mut router = Router::new();

    if api_mode == ApiMode::Legacy {
        let get_proof_gen_processor = Processor::<Locking>::new(
            blob_store.clone(),
            connection_pool.clone(),
            config.clone(),
            commitment_mode,
        );
        let submit_proof_processor = get_proof_gen_processor.clone();

        router = router.route(
            "/proof_generation_data",
            post(
                // we use post method because the returned data is not idempotent,
                // i.e we return different result on each call.
                move |_: Json<ProofGenerationDataRequest>| async move {
                    match get_proof_gen_processor.get_proof_generation_data().await {
                        Ok(data) => {
                            let response = ProofGenerationDataResponse::Success(data.map(Box::new));
                            (StatusCode::OK, Json(response)).into_response()
                        }
                        Err(e) => e.into_response(),
                    }
                },
            ),
        )
        .route(
            "/submit_proof/:l1_batch_number",
            post(
                move |l1_batch_number: Path<u32>, payload: Json<SubmitProofRequest>| async move {
                    let l1_batch_number = L1BatchNumber(l1_batch_number.0);
                    let Json(SubmitProofRequest::Proof(proof)) = payload;
                    match submit_proof_processor
                        .save_proof(l1_batch_number, (*proof).into())
                        .await
                    {
                        Ok(_) => {
                            (StatusCode::OK, Json(SubmitProofResponse::Success)).into_response()
                        }
                        Err(e) => e.into_response(),
                    }
                },
            ),
        );
    }

    if config.tee_config.tee_support {
        let get_tee_proof_gen_processor =
            TeeRequestProcessor::new(blob_store, connection_pool, config.clone(), l2_chain_id);
        let submit_tee_proof_processor = get_tee_proof_gen_processor.clone();
        let register_tee_attestation_processor = get_tee_proof_gen_processor.clone();

        router = router.route(
            "/tee/proof_inputs",
            post(
                move |payload: Json<TeeProofGenerationDataRequest>| async move {
                    let result = get_tee_proof_gen_processor
                        .get_proof_generation_data(payload)
                        .await;

                    match result {
                        Ok(Some(data)) => (StatusCode::OK, data).into_response(),
                        Ok(None) => { StatusCode::NO_CONTENT.into_response()},
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
    }

    router
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(tower_http::decompression::RequestDecompressionLayer::new().zstd(true))
}

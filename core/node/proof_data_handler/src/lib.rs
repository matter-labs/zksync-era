use std::{net::SocketAddr, sync::Arc};

use anyhow::Context as _;
use axum::{extract::Path, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use tokio::sync::watch;
use zksync_config::configs::{fri_prover_gateway::ApiMode, ProofDataHandlerConfig};
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_prover_interface::api::{
    ProofGenerationDataRequest, ProofGenerationDataResponse, SubmitProofRequest,
    SubmitProofResponse,
};
use zksync_types::{L1BatchId, L1BatchNumber, L2ChainId};

pub use crate::{
    client::ProofDataHandlerClient,
    errors::ProcessorError,
    processor::{Locking, Processor, Readonly},
};

mod client;
mod errors;
mod metrics;
pub mod node;
mod processor;

pub async fn run_server(
    config: ProofDataHandlerConfig,
    blob_store: Arc<dyn ObjectStore>,
    connection_pool: ConnectionPool<Core>,
    l2_chain_id: L2ChainId,
    api_mode: ApiMode,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let bind_address = SocketAddr::from(([0, 0, 0, 0], config.http_port));
    tracing::info!("Starting proof data handler server on {bind_address}");
    let app =
        create_proof_processing_router(blob_store, connection_pool, config, api_mode, l2_chain_id);

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
    config: ProofDataHandlerConfig,
    api_mode: ApiMode,
    l2_chain_id: L2ChainId,
) -> Router {
    let mut router = Router::new();

    if api_mode == ApiMode::Legacy {
        let get_proof_gen_processor = Processor::<Locking>::new(
            blob_store.clone(),
            connection_pool.clone(),
            config.proof_generation_timeout,
            l2_chain_id,
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
                        .save_proof(L1BatchId::new(l2_chain_id, l1_batch_number), (*proof).into())
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

    router
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(tower_http::decompression::RequestDecompressionLayer::new().zstd(true))
}

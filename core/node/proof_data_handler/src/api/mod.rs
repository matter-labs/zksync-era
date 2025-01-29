use std::{net::SocketAddr, sync::Arc};

use anyhow::Context as _;
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use tokio::sync::watch;
use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_prover_interface::api::{
    RegisterTeeAttestationRequest, RegisterTeeAttestationResponse, SubmitProofRequest,
    SubmitProofResponse, SubmitTeeProofRequest, TeeProofGenerationDataRequest,
    TeeProofGenerationDataResponse,
};
use zksync_types::{L1BatchNumber, L2ChainId};

use crate::errors::RequestProcessorError;

mod request_processor;
mod tee_request_processor;

#[derive(Debug)]
pub struct ProofDataHandlerApi {
    pub(crate) router: Router,
    port: u16,
}

impl ProofDataHandlerApi {
    pub fn new(state: RequestProcessor, port: u16) -> ProofDataHandlerApi {
        let router = Router::new()
            .route(
                "/submit_proof/:l1_batch_number",
                post(ProofDataHandlerApi::submit_proof),
            )
            .with_state(state)
            .layer(tower_http::compression::CompressionLayer::new())
            .layer(tower_http::decompression::RequestDecompressionLayer::new().zstd(true));

        Self { router, port }
    }

    pub fn new_with_tee_support(state: RequestProcessor, port: u16) -> ProofDataHandlerApi {
        let router = Router::new()
            .route(
                "/submit_proof/:l1_batch_number",
                post(ProofDataHandlerApi::submit_proof),
            )
            .route(
                "/tee/proof_inputs",
                get(ProofDataHandlerApi::get_tee_proof_generation_data),
            )
            .route(
                "/tee/submit_proofs/:l1_batch_number",
                post(ProofDataHandlerApi::submit_tee_proof),
            )
            .route(
                "/tee/register_attestation",
                post(ProofDataHandlerApi::register_tee_attestation),
            )
            .with_state(state)
            .layer(tower_http::compression::CompressionLayer::new())
            .layer(tower_http::decompression::RequestDecompressionLayer::new().zstd(true));

        Self { router, port }
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let bind_address = SocketAddr::from(([0, 0, 0, 0], self.port));
        tracing::info!("Starting proof data handler server on {bind_address}");
        let listener = tokio::net::TcpListener::bind(bind_address)
            .await
            .with_context(|| {
                format!("Failed binding proof data handler server to {bind_address}")
            })?;
        axum::serve(listener, self.router)
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

    async fn submit_proof(
        State(processor): State<RequestProcessor>,
        Path(l1_batch_number): Path<L1BatchNumber>,
        Json(payload): Json<SubmitProofRequest>,
    ) -> Result<Json<SubmitProofResponse>, RequestProcessorError> {
        processor.submit_proof(l1_batch_number, payload).await
    }

    async fn get_tee_proof_generation_data(
        State(processor): State<RequestProcessor>,
        Json(payload): Json<TeeProofGenerationDataRequest>,
    ) -> Result<Json<Option<TeeProofGenerationDataResponse>>, RequestProcessorError> {
        processor.get_tee_proof_generation_data(payload).await
    }

    async fn submit_tee_proof(
        State(processor): State<RequestProcessor>,
        Path(l1_batch_number): Path<L1BatchNumber>,
        Json(payload): Json<SubmitTeeProofRequest>,
    ) -> Result<Json<SubmitProofResponse>, RequestProcessorError> {
        processor.submit_tee_proof(l1_batch_number, payload).await
    }

    async fn register_tee_attestation(
        State(processor): State<RequestProcessor>,
        Json(payload): Json<RegisterTeeAttestationRequest>,
    ) -> Result<Json<RegisterTeeAttestationResponse>, RequestProcessorError> {
        processor.register_tee_attestation(payload).await
    }
}

#[derive(Clone)]
pub struct RequestProcessor {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Core>,
    config: ProofDataHandlerConfig,
    l2_chain_id: L2ChainId,
}

impl RequestProcessor {
    pub fn new(
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool<Core>,
        config: ProofDataHandlerConfig,
        l2_chain_id: L2ChainId,
    ) -> Self {
        Self {
            blob_store,
            pool,
            config,
            l2_chain_id,
        }
    }
}

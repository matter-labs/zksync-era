use std::{net::SocketAddr, sync::Arc};

use anyhow::Context as _;
use axum::{
    extract::{Path, Request, State},
    middleware::Next,
    routing::post,
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

use crate::{errors::RequestProcessorError, metrics::Method, middleware::MetricsMiddleware};

mod request_processor;
mod tee_request_processor;

#[derive(Debug)]
pub struct TeeProofDataHandler {
    pub(crate) router: Router,
    port: u16,
}

impl TeeProofDataHandler {
    pub fn new(state: RequestProcessor, port: u16) -> TeeProofDataHandler {
        let middleware_factory = |method: Method| {
            axum::middleware::from_fn(move |req: Request, next: Next| async move {
                let middleware = MetricsMiddleware::new(method);
                let response = next.run(req).await;
                middleware.observe(response.status());
                response
            })
        };

        let router = Router::new()
            .route(
                "/tee/proof_inputs",
                post(TeeProofDataHandler::get_tee_proof_generation_data)
                    .layer(middleware_factory(Method::GetTeeProofInputs)),
            )
            .route(
                "/tee/submit_proofs/:l1_batch_number",
                post(TeeProofDataHandler::submit_tee_proof)
                    .layer(middleware_factory(Method::TeeSubmitProofs)),
            )
            .route(
                "/tee/register_attestation",
                post(TeeProofDataHandler::register_tee_attestation)
                    .layer(middleware_factory(Method::TeeRegisterAttestation)),
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

    async fn get_tee_proof_generation_data(
        State(processor): State<RequestProcessor>,
        Json(payload): Json<TeeProofGenerationDataRequest>,
    ) -> Result<Json<Option<TeeProofGenerationDataResponse>>, RequestProcessorError> {
        match processor.get_tee_proof_generation_data(payload).await {
            Ok(Json(Some(data))) => Ok(Json(Some(data))),
            Ok(Json(None)) => Err(RequestProcessorError::NoContent(
                "get_tee_proof_generation_data".to_string(),
            )),
            Err(e) => Err(e),
        }
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

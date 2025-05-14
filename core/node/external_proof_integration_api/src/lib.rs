use std::net::SocketAddr;

use anyhow::Context;
use axum::{
    extract::{Path, Request, State},
    middleware::Next,
    routing::{get, post},
    Router,
};
use tokio::sync::watch;
use types::{ExternalProof, ProofGenerationDataResponse};
use zksync_basic_types::L1BatchNumber;
use zksync_proof_data_handler::{Processor, ProcessorError, Readonly};

use crate::{metrics::Method, middleware::MetricsMiddleware};

mod error;
mod metrics;
mod middleware;
pub mod node;
mod types;

/// External API implementation.
#[derive(Debug)]
pub struct Api {
    router: Router,
    port: u16,
}

impl Api {
    pub fn new(processor: Processor<Readonly>, port: u16) -> Self {
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
                "/proof_generation_data",
                get(Api::latest_generation_data)
                    .layer(middleware_factory(Method::GetLatestProofGenerationData)),
            )
            .route(
                "/proof_generation_data/:l1_batch_number",
                get(Api::generation_data_for_existing_batch)
                    .layer(middleware_factory(Method::GetSpecificProofGenerationData)),
            )
            .route(
                "/verify_proof/:l1_batch_number",
                post(Api::verify_proof).layer(middleware_factory(Method::VerifyProof)),
            )
            .with_state(processor);

        Self { router, port }
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let bind_address = SocketAddr::from(([0, 0, 0, 0], self.port));
        tracing::info!("Starting external prover API server on {bind_address}");

        let listener = tokio::net::TcpListener::bind(bind_address)
            .await
            .with_context(|| {
                format!("Failed binding external prover API server to {bind_address}")
            })?;
        axum::serve(listener, self.router)
        .with_graceful_shutdown(async move {
            if stop_receiver.changed().await.is_err() {
                tracing::warn!("Stop request sender for external prover API server was dropped without sending a signal");
            }
            tracing::info!("Stop request received, external prover API server is shutting down");
        })
        .await
        .context("External prover API server failed")?;
        tracing::info!("External prover API server shut down");
        Ok(())
    }

    async fn latest_generation_data(
        State(processor): State<Processor<Readonly>>,
    ) -> Result<ProofGenerationDataResponse, ProcessorError> {
        let data = processor.get_proof_generation_data().await?;
        Ok(ProofGenerationDataResponse(data))
    }

    async fn generation_data_for_existing_batch(
        State(processor): State<Processor<Readonly>>,
        Path(l1_batch_number): Path<u32>,
    ) -> Result<ProofGenerationDataResponse, ProcessorError> {
        let data = processor
            .proof_generation_data_for_existing_batch(L1BatchNumber(l1_batch_number))
            .await?;

        Ok(ProofGenerationDataResponse(data))
    }

    async fn verify_proof(
        State(processor): State<Processor<Readonly>>,
        Path(l1_batch_number): Path<u32>,
        proof: ExternalProof,
    ) -> Result<(), ProcessorError> {
        processor
            .verify_proof(
                L1BatchNumber(l1_batch_number),
                proof.raw(),
                proof.protocol_version(),
            )
            .await
    }
}

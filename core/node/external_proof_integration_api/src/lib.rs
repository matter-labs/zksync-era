mod error;
mod metrics;
mod middleware;
mod processor;
mod types;

use std::net::SocketAddr;

use anyhow::Context;
use axum::{
    extract::{Path, Request, State},
    middleware::Next,
    routing::{get, post},
    Router,
};
use error::ProcessorError;
use tokio::sync::watch;
use types::{ExternalProof, ProofGenerationDataResponse};
use zksync_basic_types::L1BatchNumber;

pub use crate::processor::Processor;
use crate::{
    metrics::{CallOutcome, Method},
    middleware::MetricsMiddleware,
};

/// External API implementation.
#[derive(Debug)]
pub struct Api {
    router: Router,
    port: u16,
}

impl Api {
    pub fn new(processor: Processor, port: u16) -> Self {
        let middleware_factory = |method: Method| {
            axum::middleware::from_fn(move |req: Request, next: Next| async move {
                let middleware = MetricsMiddleware::new(method);
                let response = next.run(req).await;
                let outcome = match response.status().is_success() {
                    true => CallOutcome::Success,
                    false => CallOutcome::Failure,
                };
                middleware.observe(outcome);
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
                tracing::warn!("Stop signal sender for external prover API server was dropped without sending a signal");
            }
            tracing::info!("Stop signal received, external prover API server is shutting down");
        })
        .await
        .context("External prover API server failed")?;
        tracing::info!("External prover API server shut down");
        Ok(())
    }

    async fn latest_generation_data(
        State(processor): State<Processor>,
    ) -> Result<ProofGenerationDataResponse, ProcessorError> {
        processor.get_proof_generation_data().await
    }

    async fn generation_data_for_existing_batch(
        State(processor): State<Processor>,
        Path(l1_batch_number): Path<u32>,
    ) -> Result<ProofGenerationDataResponse, ProcessorError> {
        processor
            .proof_generation_data_for_existing_batch(L1BatchNumber(l1_batch_number))
            .await
    }

    async fn verify_proof(
        State(processor): State<Processor>,
        Path(l1_batch_number): Path<u32>,
        proof: ExternalProof,
    ) -> Result<(), ProcessorError> {
        processor
            .verify_proof(L1BatchNumber(l1_batch_number), proof)
            .await
    }
}

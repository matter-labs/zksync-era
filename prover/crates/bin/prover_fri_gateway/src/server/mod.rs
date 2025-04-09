use std::net::SocketAddr;

use axum::{extract::State, Router};
use processor::Processor;
use tokio::sync::watch;
use anyhow::Context as _;
use zksync_prover_interface::api::{ProofGenerationData, SubmitProofRequest};
use zksync_types::L1BatchNumber;

mod processor;
mod error;

pub struct Api {
    router: Router,
    port: u16,
}

impl Api {
    pub fn new(processor: Processor, port: u16) -> Self {
        let router = Router::new()
            .route("/next_proof", axum::routing::get(Self::next_proof))
            .route(
                "/save_proof_gen_data",
                axum::routing::post(Self::save_proof_gen_data),
            )
            .route(
                "/save_successful_sent_proof",
                axum::routing::post(Self::save_successful_sent_proof),
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

    async fn next_proof(
        State(processor): State<Processor>,
    ) -> anyhow::Result<Option<(L1BatchNumber, SubmitProofRequest)>> {
        Ok(processor.next_submit_proof_request().await)
    }

    async fn save_proof_gen_data(
        State(processor): State<Processor>,
        data: ProofGenerationData,
    ) -> anyhow::Result<()> {
        Ok(processor.save_proof_gen_data(data).await)
    }

    async fn save_successful_sent_proof(
        State(processor): State<Processor>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        Ok(processor.save_successful_sent_proof(l1_batch_number).await)
    }
}
use std::net::SocketAddr;

use axum::{extract::State, Router};
use processor::Processor;
use tokio::sync::watch;
use anyhow::Context as _;
use zksync_prover_interface::api::SubmitProofRequest;

mod processor;

pub struct Api {
    router: Router,
    port: u16,
}

impl Api {
    pub fn new(processor: Processor, port: u16) -> Self {
        let router = Router::new();
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
    ) -> anyhow::Result<(L1BatchNumber, SubmitProofRequest)> {
        processor.next_submit_proof_request().await
    }
}
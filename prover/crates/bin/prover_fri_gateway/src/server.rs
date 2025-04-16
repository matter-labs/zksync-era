use std::net::SocketAddr;

use anyhow::Context as _;
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use tokio::sync::watch;
use zksync_prover_interface::api::{GetNextProofResponse, ProofGenerationData};
use zksync_types::L1BatchNumber;

use crate::{error::ProcessorError, proof_data_manager::ProofDataManager};

pub struct Api {
    router: Router,
    port: u16,
}

impl Api {
    pub fn new(processor: ProofDataManager, port: u16) -> Self {
        let router = Router::new()
            .route("/poll_generated_proofs", get(Api::get_next_proof))
            .route(
                "/submit_prove_request",
                post(Api::save_proof_generation_data),
            )
            .route(
                "/acknowledge_received_proof",
                post(Api::save_successful_sent_proof),
            )
            .with_state(processor);

        Self { router, port }
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let bind_address = SocketAddr::from(([0, 0, 0, 0], self.port));
        tracing::info!("Starting prover gateway API server on {bind_address}");

        let listener = tokio::net::TcpListener::bind(bind_address)
            .await
            .with_context(|| {
                format!("Failed binding prover gateway API server to {bind_address}")
            })?;

        axum::serve(listener, self.router)
        .with_graceful_shutdown(async move {
            if stop_receiver.changed().await.is_err() {
                tracing::warn!("Stop signal sender for prover gateway API server was dropped without sending a signal");
            }
            tracing::info!("Stop signal received, prover gateway API server is shutting down");
        })
        .await
        .context("Prover gateway API server failed")?;
        tracing::info!("Prover gateway API server shut down");
        Ok(())
    }

    async fn get_next_proof(
        State(processor): State<ProofDataManager>,
    ) -> Result<Json<Option<GetNextProofResponse>>, ProcessorError> {
        let proof = processor.get_next_proof().await;

        if let Some((l1_batch_number, proof)) = proof? {
            let response = GetNextProofResponse {
                l1_batch_number,
                proof: proof.into(),
            };

            Ok(Json(Some(response)))
        } else {
            Ok(Json(None))
        }
    }

    async fn save_proof_generation_data(
        State(processor): State<ProofDataManager>,
        Json(data): Json<ProofGenerationData>,
    ) -> Result<(), ProcessorError> {
        processor.save_proof_gen_data(data).await
    }

    async fn save_successful_sent_proof(
        State(processor): State<ProofDataManager>,
        Json(l1_batch_number): Json<L1BatchNumber>,
    ) -> Result<(), ProcessorError> {
        processor.save_successful_sent_proof(l1_batch_number).await
    }
}

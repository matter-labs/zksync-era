use std::net::SocketAddr;

use anyhow::Context as _;
use axum::{
    extract::{DefaultBodyLimit, State},
    routing::post,
    Json, Router,
};
use tokio::sync::watch;
use tower_http::trace::TraceLayer;
use zksync_prover_interface::api::{
    PollGeneratedProofsRequest, PollGeneratedProofsResponse, ProofGenerationData,
    SubmitProofGenerationDataResponse,
};

use crate::{error::ProcessorError, proof_data_manager::ProofDataManager};

pub struct Api {
    router: Router,
    port: u16,
}

impl Api {
    pub fn new(processor: ProofDataManager, port: u16) -> Self {
        let router = Router::new()
            .route("/poll_generated_proofs", post(Api::get_generated_proofs))
            .route(
                "/submit_request_for_proofs",
                post(Api::save_proof_generation_data),
            )
            .layer(TraceLayer::new_for_http())
            .layer(DefaultBodyLimit::disable())
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
                tracing::warn!("Stop request sender for prover gateway API server was dropped without sending a signal");
            }
            tracing::info!("Stop request received, prover gateway API server is shutting down");
        })
        .await
        .context("Prover gateway API server failed")?;
        tracing::info!("Prover gateway API server shut down");
        Ok(())
    }

    async fn get_generated_proofs(
        State(processor): State<ProofDataManager>,
        Json(request): Json<PollGeneratedProofsRequest>,
    ) -> Result<Json<Option<PollGeneratedProofsResponse>>, ProcessorError> {
        tracing::info!("Received request for proof: {:?}", request);

        let proof = processor.get_proof_for_batch(request.l1_batch_id).await?;

        let response = proof.map(|proof| PollGeneratedProofsResponse {
            l1_batch_id: request.l1_batch_id,
            proof: proof.into(),
        });

        if response.is_none() {
            tracing::info!("Proof for batch {} is not ready yet", request.l1_batch_id);
        } else {
            tracing::info!("Proof is ready for batch {}", request.l1_batch_id);
        }

        Ok(Json(response))
    }

    async fn save_proof_generation_data(
        State(processor): State<ProofDataManager>,
        Json(data): Json<ProofGenerationData>,
    ) -> Result<Json<SubmitProofGenerationDataResponse>, ProcessorError> {
        tracing::info!(
            "Received proof generation data for batch {}, chain_id {}",
            data.l1_batch_number,
            data.chain_id
        );
        processor.save_proof_gen_data(data).await?;

        Ok(Json(SubmitProofGenerationDataResponse))
    }
}

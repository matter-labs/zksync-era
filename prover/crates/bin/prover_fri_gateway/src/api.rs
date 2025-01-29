use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use tokio::sync::watch;
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_prover_dal::{ConnectionPool, DalError, Prover, ProverDal};
use zksync_prover_interface::api::ProofGenerationData;

pub(crate) struct ProverGatewayApi {
    router: Router,
    port: u16,
}

impl ProverGatewayApi {
    pub fn new(port: u16, state: Processor) -> ProverGatewayApi {
        let router = Router::new()
            .route(
                "/proof_generation_data",
                post(ProverGatewayApi::submit_proof_generation_data),
            )
            .with_state(state);

        Self { router, port }
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let bind_address = SocketAddr::from(([0, 0, 0, 0], self.port));
        tracing::info!("Starting prover gateway server on {bind_address}");
        let listener = tokio::net::TcpListener::bind(bind_address)
            .await
            .with_context(|| format!("Failed binding prover gateway server to {bind_address}"))?;
        axum::serve(listener, self.router)
            .with_graceful_shutdown(async move {
                if stop_receiver.changed().await.is_err() {
                    tracing::warn!("Stop signal sender for prover gateway server was dropped without sending a signal");
                }
                tracing::info!("Stop signal received, prover gateway server is shutting down");
            })
            .await
            .context("Prover gateway server failed")?;
        tracing::info!("Prover gateway server shut down");
        Ok(())
    }

    async fn submit_proof_generation_data(
        State(processor): State<Processor>,
        Json(payload): Json<ProofGenerationData>,
    ) -> Result<(), ProcessorError> {
        processor.save_proof_gen_data(payload).await
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Processor {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Prover>,
}

impl Processor {
    pub(crate) fn new(blob_store: Arc<dyn ObjectStore>, pool: ConnectionPool<Prover>) -> Self {
        Self { blob_store, pool }
    }

    pub(crate) async fn save_proof_gen_data(
        &self,
        data: ProofGenerationData,
    ) -> Result<(), ProcessorError> {
        tracing::info!(
            "Received proof generation data for batch: {:?}",
            data.l1_batch_number
        );

        let store = &*self.blob_store;
        let witness_inputs = store
            .put(data.l1_batch_number, &data.witness_input_data)
            .await?;
        let mut connection = self.pool.connection().await?;

        connection
            .fri_protocol_versions_dal()
            .save_prover_protocol_version(data.protocol_version, data.l1_verifier_config)
            .await;

        connection
            .fri_witness_generator_dal()
            .save_witness_inputs(data.l1_batch_number, &witness_inputs, data.protocol_version)
            .await;
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) enum ProcessorError {
    ObjectStore(ObjectStoreError),
    Dal(DalError),
}

impl From<DalError> for ProcessorError {
    fn from(err: DalError) -> Self {
        ProcessorError::Dal(err)
    }
}

impl From<ObjectStoreError> for ProcessorError {
    fn from(err: ObjectStoreError) -> Self {
        ProcessorError::ObjectStore(err)
    }
}

impl IntoResponse for ProcessorError {
    fn into_response(self) -> Response {
        let (status_code, message) = match self {
            Self::ObjectStore(err) => {
                tracing::error!("GCS error: {:?}", err);
                (
                    StatusCode::BAD_GATEWAY,
                    "Failed fetching/saving from GCS".to_owned(),
                )
            }
            Self::Dal(err) => {
                tracing::error!("Sqlx error: {:?}", err);
                (
                    StatusCode::BAD_GATEWAY,
                    "Failed fetching/saving from db".to_owned(),
                )
            }
        };
        (status_code, message).into_response()
    }
}

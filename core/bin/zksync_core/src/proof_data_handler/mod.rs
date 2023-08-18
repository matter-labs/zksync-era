use axum::extract::Path;
use axum::{routing::post, Json, Router};
use std::net::SocketAddr;
use tokio::sync::watch;

use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStore;
use zksync_types::prover_server_api::{ProofGenerationDataRequest, SubmitProofRequest};

use crate::proof_data_handler::request_processor::RequestProcessor;

mod request_processor;

pub(crate) async fn run_server(
    config: ProofDataHandlerConfig,
    blob_store: Box<dyn ObjectStore>,
    pool: ConnectionPool,
    mut stop_receiver: watch::Receiver<bool>,
) {
    let bind_address = SocketAddr::from(([0, 0, 0, 0], config.http_port));
    vlog::debug!("Starting proof data handler server on {bind_address}");

    let get_proof_gen_processor =
        RequestProcessor::new(blob_store, pool, config.proof_generation_timeout());
    let submit_proof_processor = get_proof_gen_processor.clone();
    let app = Router::new()
        .route(
            "/proof_generation_data",
            post(
                // we use post method because the returned data is not idempotent,
                // i.e we return different result on each call.
                move |payload: Json<ProofGenerationDataRequest>| async move {
                    get_proof_gen_processor
                        .get_proof_generation_data(payload)
                        .await
                },
            ),
        )
        .route(
            "/submit_proof/:l1_batch_number",
            post(
                move |l1_batch_number: Path<u32>, payload: Json<SubmitProofRequest>| async move {
                    submit_proof_processor
                        .submit_proof(l1_batch_number, payload)
                        .await
                },
            ),
        );

    axum::Server::bind(&bind_address)
        .serve(app.into_make_service())
        .with_graceful_shutdown(async move {
            if stop_receiver.changed().await.is_err() {
                vlog::warn!("Stop signal sender for proof data handler server was dropped without sending a signal");
            }
            vlog::info!("Stop signal received, proof data handler server is shutting down");
        })
        .await
        .expect("Proof data handler server failed");
    vlog::info!("Proof data handler server shut down");
}

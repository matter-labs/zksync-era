use crate::proof_data_handler::request_processor::RequestProcessor;
use anyhow::Context as _;
use axum::extract::Path;
use axum::{routing::post, Json, Router};
use std::net::SocketAddr;
use tokio::sync::watch;
use zksync_config::{
    configs::{proof_data_handler::ProtocolVersionLoadingMode, ProofDataHandlerConfig},
    ContractsConfig,
};
use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStore;
use zksync_types::{
    protocol_version::{L1VerifierConfig, VerifierParams},
    prover_server_api::{ProofGenerationDataRequest, SubmitProofRequest},
    H256,
};

mod request_processor;

fn fri_l1_verifier_config_from_env() -> anyhow::Result<L1VerifierConfig> {
    let config = ContractsConfig::from_env().context("ContractsConfig::from_env()")?;
    Ok(L1VerifierConfig {
        params: VerifierParams {
            recursion_node_level_vk_hash: config.fri_recursion_node_level_vk_hash,
            recursion_leaf_level_vk_hash: config.fri_recursion_leaf_level_vk_hash,
            // The base layer commitment is not used in the FRI prover verification.
            recursion_circuits_set_vks_hash: H256::zero(),
        },
        recursion_scheduler_level_vk_hash: config.fri_recursion_scheduler_level_vk_hash,
    })
}

pub(crate) async fn run_server(
    config: ProofDataHandlerConfig,
    blob_store: Box<dyn ObjectStore>,
    pool: ConnectionPool,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let bind_address = SocketAddr::from(([0, 0, 0, 0], config.http_port));
    tracing::debug!("Starting proof data handler server on {bind_address}");
    let l1_verifier_config: Option<L1VerifierConfig> = match config.protocol_version_loading_mode {
        ProtocolVersionLoadingMode::FromDb => None,
        ProtocolVersionLoadingMode::FromEnvVar => {
            Some(fri_l1_verifier_config_from_env().context("fri_l1_verified_config_from_env()")?)
        }
    };
    let get_proof_gen_processor =
        RequestProcessor::new(blob_store, pool, config, l1_verifier_config);
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
                tracing::warn!("Stop signal sender for proof data handler server was dropped without sending a signal");
            }
            tracing::info!("Stop signal received, proof data handler server is shutting down");
        })
        .await
        .context("Proof data handler server failed")?;
    tracing::info!("Proof data handler server shut down");
    Ok(())
}

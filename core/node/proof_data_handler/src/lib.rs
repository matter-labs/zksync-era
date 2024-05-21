use std::{net::SocketAddr, sync::Arc};

use anyhow::Context as _;
use axum::{extract::Path, routing::post, Json, Router};
use tokio::sync::watch;
use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_prover_interface::api::{
    ProofGenerationDataRequest, SubmitProofRequest, SubmitTeeProofRequest,
    TeeProofGenerationDataRequest,
};
use zksync_types::commitment::L1BatchCommitmentMode;

use crate::request_processor::RequestProcessor;
use crate::tee_request_processor::TeeRequestProcessor;

mod errors;
mod request_processor;
mod tee_request_processor;

pub async fn run_server(
    config: ProofDataHandlerConfig,
    blob_store: Arc<dyn ObjectStore>,
    connection_pool: ConnectionPool<Core>,
    commitment_mode: L1BatchCommitmentMode,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let bind_address = SocketAddr::from(([0, 0, 0, 0], config.http_port));
    tracing::debug!("Starting proof data handler server on {bind_address}");
    let app = create_proof_processing_router(blob_store, connection_pool, config, commitment_mode);

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

fn create_proof_processing_router(
    blob_store: Arc<dyn ObjectStore>,
    connection_pool: ConnectionPool<Core>,
    config: ProofDataHandlerConfig,
    commitment_mode: L1BatchCommitmentMode,
) -> Router {
    let get_tee_proof_gen_processor =
        TeeRequestProcessor::new(blob_store.clone(), connection_pool.clone(), config.clone());
    let submit_tee_proof_processor = get_tee_proof_gen_processor.clone();
    let get_proof_gen_processor =
        RequestProcessor::new(blob_store, connection_pool, config, commitment_mode);
    let submit_proof_processor = get_proof_gen_processor.clone();

    Router::new()
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
        )
        .route(
            "/tee_proof_generation_data",
            post(
                move |payload: Json<TeeProofGenerationDataRequest>| async move {
                    get_tee_proof_gen_processor
                        .get_proof_generation_data(payload)
                        .await
                },
            ),
        )
        .route(
            "/submit_tee_proof/:l1_batch_number", // add TEE type as a parameter (and pubkey?)
            post(
                move |l1_batch_number: Path<u32>, payload: Json<SubmitTeeProofRequest>| async move {
                    submit_tee_proof_processor
                        .submit_proof(l1_batch_number, payload)
                        .await
                },
            ),
        )
}

#[cfg(test)]
mod tests {
    use crate::{create_proof_processing_router, ConnectionPool};
    use axum::{
        body::Body,
        http::{self, Method, Request, StatusCode},
    };
    use multivm::interface::{L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode};
    use serde_json::json;
    use tower::ServiceExt;
    use zksync_basic_types::U256;
    use zksync_config::configs::ProofDataHandlerConfig;
    use zksync_contracts::{BaseSystemContracts, SystemContractCode};
    use zksync_object_store::ObjectStoreFactory;
    use zksync_prover_interface::inputs::PrepareBasicCircuitsJob;
    use zksync_tee_verifier::TeeVerifierInput;
    use zksync_types::{commitment::L1BatchCommitmentMode, L1BatchNumber, H256};

    #[tokio::test]
    async fn request_tee_proof_generation_data() {
        let batch_number = L1BatchNumber::from(1);
        let tvi = TeeVerifierInput::new(
            PrepareBasicCircuitsJob::new(0),
            vec![],
            L1BatchEnv {
                previous_batch_hash: Some(H256([1; 32])),
                number: batch_number,
                timestamp: 0,
                fee_input: Default::default(),
                fee_account: Default::default(),
                enforced_base_fee: None,
                first_l2_block: L2BlockEnv {
                    number: 0,
                    timestamp: 0,
                    prev_block_hash: H256([1; 32]),
                    max_virtual_blocks_to_create: 0,
                },
            },
            SystemEnv {
                zk_porter_available: false,
                version: Default::default(),
                base_system_smart_contracts: BaseSystemContracts {
                    bootloader: SystemContractCode {
                        code: vec![U256([1; 4])],
                        hash: H256([1; 32]),
                    },
                    default_aa: SystemContractCode {
                        code: vec![U256([1; 4])],
                        hash: H256([1; 32]),
                    },
                },
                bootloader_gas_limit: 0,
                execution_mode: TxExecutionMode::VerifyExecute,
                default_validation_computational_gas_limit: 0,
                chain_id: Default::default(),
            },
            vec![(H256([1; 32]), vec![0, 1, 2, 3, 4])],
        );
        let blob_store = ObjectStoreFactory::mock().create_store().await;
        blob_store.put(batch_number, &tvi).await.unwrap();
        // TODO mock relevant SQL table once the logic is implemented in the TeeRequestProcessor::get_proof_generation_data
        // TODO useful examples: https://github.com/tokio-rs/axum/blob/main/examples/testing/src/main.rs#L58
        let connection_pool = ConnectionPool::test_pool().await;
        let app = create_proof_processing_router(
            blob_store,
            connection_pool,
            ProofDataHandlerConfig {
                http_port: 1337,
                proof_generation_timeout_in_secs: 10,
            },
            L1BatchCommitmentMode::Rollup,
        );
        let req_body = Body::from(serde_json::to_vec(&json!({})).unwrap());
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/tee_proof_generation_data")
                    .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                    .body(req_body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let json = json
            .get("Success")
            .expect("Unexpected response format")
            .clone();
        let deserialized: TeeVerifierInput = serde_json::from_value(json).unwrap();
        assert_eq!(tvi, deserialized);
    }
}

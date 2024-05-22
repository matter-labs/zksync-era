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

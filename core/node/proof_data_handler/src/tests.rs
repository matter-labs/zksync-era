use std::time::Instant;

use axum::{
    body::Body,
    http::{self, Method, Request, StatusCode},
    response::Response,
    Router,
};
use hyper::body::HttpBody;
use serde_json::json;
use tower::ServiceExt;
use zksync_basic_types::U256;
use zksync_config::configs::ProofDataHandlerConfig;
use zksync_contracts::{BaseSystemContracts, SystemContractCode};
use zksync_dal::{ConnectionPool, CoreDal};
use zksync_multivm::interface::{L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode};
use zksync_object_store::MockObjectStore;
use zksync_prover_interface::{api::SubmitTeeProofRequest, inputs::PrepareBasicCircuitsJob};
use zksync_tee_verifier::TeeVerifierInput;
use zksync_types::{commitment::L1BatchCommitmentMode, L1BatchNumber, H256};

use crate::create_proof_processing_router;

// Test the /tee/proof_inputs endpoint by:
// 1. Mocking an object store with a single batch blob containing TEE verifier input
// 2. Populating the SQL db with relevant information about the status of the TEE verifier input and
//    TEE proof generation
// 3. Sending a request to the /tee/proof_inputs endpoint and asserting that the response
//    matches the file from the object store
#[tokio::test]
async fn request_tee_proof_inputs() {
    // prepare a sample mocked TEE verifier input

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

    // populate mocked object store with a single batch blob

    let blob_store = MockObjectStore::arc();
    let object_path = blob_store.put(batch_number, &tvi).await.unwrap();

    // get connection to the SQL db and mock the status of the TEE proof generation

    let db_conn_pool = ConnectionPool::test_pool().await;
    mock_tee_batch_status(db_conn_pool.clone(), batch_number, &object_path).await;

    // test the /tee/proof_inputs endpoint; it should return the batch from the object store

    let app = create_proof_processing_router(
        blob_store,
        db_conn_pool,
        ProofDataHandlerConfig {
            http_port: 1337,
            proof_generation_timeout_in_secs: 10,
            tee_support: true,
        },
        L1BatchCommitmentMode::Rollup,
    );
    let req_body = Body::from(serde_json::to_vec(&json!({})).unwrap());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/tee/proof_inputs")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(req_body)
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let json = json
        .get("Success")
        .expect("Unexpected response format")
        .clone();
    let deserialized: TeeVerifierInput = serde_json::from_value(json).unwrap();

    assert_eq!(tvi, deserialized);
}

// Test /tee/submit_proofs endpoint using a mocked TEE proof and verify response and db state
#[tokio::test]
async fn submit_tee_proof() {
    let blob_store = MockObjectStore::arc();
    let db_conn_pool = ConnectionPool::test_pool().await;
    let object_path = "mocked_object_path";
    let batch_number = L1BatchNumber::from(1);

    mock_tee_batch_status(db_conn_pool.clone(), batch_number, object_path).await;

    // send a request to the /tee/submit_proofs endpoint, using a mocked TEE proof

    let tee_proof_request_str = r#"{
        "signature": [ 0, 1, 2, 3, 4 ],
        "pubkey": [ 5, 6, 7, 8, 9 ],
        "proof": [ 10, 11, 12, 13, 14 ]
    }"#;
    let tee_proof_request =
        serde_json::from_str::<SubmitTeeProofRequest>(tee_proof_request_str).unwrap();
    let uri = format!("/tee/submit_proofs/{}", batch_number.0);
    let app = create_proof_processing_router(
        blob_store,
        db_conn_pool.clone(),
        ProofDataHandlerConfig {
            http_port: 1337,
            proof_generation_timeout_in_secs: 10,
            tee_support: true,
        },
        L1BatchCommitmentMode::Rollup,
    );

    // this should fail because we haven't saved the attestation for the pubkey yet

    let response = send_submit_tee_proof_request(&app, &uri, &tee_proof_request).await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

    // save the attestation for the pubkey

    let attestation = [15, 16, 17, 18, 19];
    let mut proof_dal = db_conn_pool.connection().await.unwrap();
    proof_dal
        .tee_proof_generation_dal()
        .save_attestation(&tee_proof_request.0.pubkey, &attestation)
        .await
        .expect("Failed to save attestation");

    // resend the same request; this time, it should be successful.

    let response = send_submit_tee_proof_request(&app, &uri, &tee_proof_request).await;
    assert_eq!(response.status(), StatusCode::OK);

    // there should not be any batches awaiting proof in the db anymore

    let mut proof_db_conn = db_conn_pool.connection().await.unwrap();
    let oldest_batch_number = proof_db_conn
        .tee_proof_generation_dal()
        .get_oldest_unpicked_batch()
        .await
        .unwrap();

    assert!(oldest_batch_number.is_none());
}

// Mock SQL db with information about the status of the TEE proof generation
async fn mock_tee_batch_status(
    db_conn_pool: ConnectionPool<zksync_dal::Core>,
    batch_number: L1BatchNumber,
    object_path: &str,
) {
    let mut proof_db_conn = db_conn_pool.connection().await.unwrap();
    let mut proof_dal = proof_db_conn.tee_proof_generation_dal();
    let mut input_db_conn = db_conn_pool.connection().await.unwrap();
    let mut input_producer_dal = input_db_conn.tee_verifier_input_producer_dal();

    // there should not be any batches awaiting proof in the db yet

    let oldest_batch_number = proof_dal.get_oldest_unpicked_batch().await.unwrap();
    assert!(oldest_batch_number.is_none());

    // mock SQL table with relevant information about the status of the TEE verifier input

    input_producer_dal
        .create_tee_verifier_input_producer_job(batch_number)
        .await
        .expect("Failed to create tee_verifier_input_producer_job");

    // pretend that the TEE verifier input blob file was fetched successfully

    input_producer_dal
        .mark_job_as_successful(batch_number, Instant::now(), object_path)
        .await
        .expect("Failed to mark tee_verifier_input_producer_job job as successful");

    // mock SQL table with relevant information about the status of TEE proof generation ('ready_to_be_proven')

    proof_dal
        .insert_tee_proof_generation_job(batch_number)
        .await
        .expect("Failed to insert tee_proof_generation_job");

    // now, there should be one batch in the db awaiting proof

    let oldest_batch_number = proof_dal
        .get_oldest_unpicked_batch()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(oldest_batch_number, batch_number);
}

async fn send_submit_tee_proof_request(
    app: &Router,
    uri: &str,
    tee_proof_request: &SubmitTeeProofRequest,
) -> Response {
    let req_body = Body::from(serde_json::to_vec(tee_proof_request).unwrap());
    app.clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(uri)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(req_body)
                .unwrap(),
        )
        .await
        .unwrap()
}

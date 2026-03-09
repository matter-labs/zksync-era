use std::time::Duration;

use axum::{
    body::{to_bytes, Body},
    http::{self, Method, Request, StatusCode},
    response::Response,
    Router,
};
use serde_json::json;
use tower::ServiceExt;
use zksync_config::configs::TeeProofDataHandlerConfig;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::MockObjectStore;
use zksync_tee_prover_interface::api::SubmitTeeProofRequest;
use zksync_types::{
    block::L1BatchHeader, commitment::L1BatchCommitmentMode, tee_types::TeeType, L1BatchNumber,
    L2ChainId, ProtocolVersion, ProtocolVersionId, H256,
};

use crate::create_proof_processing_router;

fn test_config() -> TeeProofDataHandlerConfig {
    TeeProofDataHandlerConfig {
        http_port: 1337,
        first_processed_batch: L1BatchNumber(0),
        proof_generation_timeout: Duration::from_secs(600),
        batch_permanently_ignored_timeout: Duration::from_secs(10 * 24 * 3_600),
    }
}

#[tokio::test]
async fn request_tee_proof_inputs() {
    let db_conn_pool = ConnectionPool::test_pool().await;

    let app = create_proof_processing_router(
        MockObjectStore::arc(),
        db_conn_pool.clone(),
        test_config(),
        L1BatchCommitmentMode::Rollup,
        L2ChainId::default(),
    );
    let test_cases = vec![
        (json!({ "tee_type": "sgx" }), StatusCode::NO_CONTENT),
        (
            json!({ "tee_type": "Sgx" }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ];

    for (body, expected_status) in test_cases {
        let req_body = Body::from(serde_json::to_vec(&body).unwrap());
        let response = app
            .clone()
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

        assert_eq!(response.status(), expected_status);
    }
}

#[tokio::test]
async fn request_tee_proof_inputs_no_lock_returns_404_for_missing_batch() {
    let db_conn_pool = ConnectionPool::test_pool().await;
    let app = create_proof_processing_router(
        MockObjectStore::arc(),
        db_conn_pool,
        test_config(),
        L1BatchCommitmentMode::Rollup,
        L2ChainId::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/tee/proof_inputs_no_lock/1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn request_tee_proof_inputs_no_lock_returns_404_for_missing_blob_data() {
    let db_conn_pool = ConnectionPool::test_pool().await;
    save_default_protocol_version(&db_conn_pool).await;
    insert_batch_for_tee_inputs(db_conn_pool.clone(), L1BatchNumber(1), true).await;

    let app = create_proof_processing_router(
        MockObjectStore::arc(),
        db_conn_pool,
        test_config(),
        L1BatchCommitmentMode::Rollup,
        L2ChainId::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/tee/proof_inputs_no_lock/1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn present_batches_returns_null_fields_when_empty() {
    let db_conn_pool = ConnectionPool::test_pool().await;
    let app = create_proof_processing_router(
        MockObjectStore::arc(),
        db_conn_pool,
        test_config(),
        L1BatchCommitmentMode::Rollup,
        L2ChainId::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/tee/present_batches")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response_json(response).await;
    assert_eq!(body, json!({ "oldest_batch": null, "latest_batch": null }));
}

#[tokio::test]
async fn present_batches_returns_oldest_and_latest_batches() {
    let db_conn_pool = ConnectionPool::test_pool().await;
    save_default_protocol_version(&db_conn_pool).await;
    insert_batch_for_tee_inputs(db_conn_pool.clone(), L1BatchNumber(1), true).await;
    insert_batch_for_tee_inputs(db_conn_pool.clone(), L1BatchNumber(3), false).await;
    insert_batch_for_tee_inputs(db_conn_pool.clone(), L1BatchNumber(5), true).await;

    let app = create_proof_processing_router(
        MockObjectStore::arc(),
        db_conn_pool,
        test_config(),
        L1BatchCommitmentMode::Rollup,
        L2ChainId::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/tee/present_batches")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response_json(response).await;
    assert_eq!(body, json!({ "oldest_batch": 1, "latest_batch": 5 }));
}

// Test /tee/submit_proofs endpoint using a mocked TEE proof and verify response and db state
#[tokio::test]
async fn submit_tee_proof() {
    let batch_number = L1BatchNumber::from(1);
    let db_conn_pool = ConnectionPool::test_pool().await;

    mock_tee_batch_status(db_conn_pool.clone(), batch_number).await;

    let tee_proof_request_str = r#"{
        "signature": "0001020304",
        "pubkey": "0506070809",
        "proof": "0A0B0C0D0E",
        "tee_type": "sgx"
    }"#;
    let tee_proof_request =
        serde_json::from_str::<SubmitTeeProofRequest>(tee_proof_request_str).unwrap();
    let uri = format!("/tee/submit_proofs/{}", batch_number.0);
    let app = create_proof_processing_router(
        MockObjectStore::arc(),
        db_conn_pool.clone(),
        test_config(),
        L1BatchCommitmentMode::Rollup,
        L2ChainId::default(),
    );

    // this should fail because we haven't saved the attestation for the pubkey yet

    let response = send_submit_tee_proof_request(&app, &uri, &tee_proof_request).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    // save the attestation for the pubkey

    let attestation = [15, 16, 17, 18, 19];
    let mut proof_dal = db_conn_pool.connection().await.unwrap();
    proof_dal
        .tee_proof_generation_dal()
        .save_attestation(&tee_proof_request.0.pubkey, &attestation)
        .await
        .expect("Failed to save attestation");

    // resend the same request; this time, it should be successful

    let response = send_submit_tee_proof_request(&app, &uri, &tee_proof_request).await;
    assert_eq!(response.status(), StatusCode::OK);

    // there should not be any batches awaiting proof in the db anymore

    let mut proof_db_conn = db_conn_pool.connection().await.unwrap();
    let oldest_batch_number = proof_db_conn
        .tee_proof_generation_dal()
        .get_oldest_picked_by_prover_batch()
        .await
        .unwrap();

    assert!(oldest_batch_number.is_none());

    // there should be one SGX proof in the db now

    let proofs = proof_db_conn
        .tee_proof_generation_dal()
        .get_tee_proofs(batch_number, Some(TeeType::Sgx))
        .await
        .unwrap();

    assert_eq!(proofs.len(), 1);

    let proof = &proofs[0];

    assert_eq!(proof.proof.as_ref().unwrap(), &tee_proof_request.0.proof);
    assert_eq!(proof.attestation.as_ref().unwrap(), &attestation);
    assert_eq!(
        proof.signature.as_ref().unwrap(),
        &tee_proof_request.0.signature
    );
    assert_eq!(proof.pubkey.as_ref().unwrap(), &tee_proof_request.0.pubkey);
}

// Mock SQL db with information about the status of the TEE proof generation
async fn mock_tee_batch_status(
    db_conn_pool: ConnectionPool<zksync_dal::Core>,
    batch_number: L1BatchNumber,
) {
    let mut proof_db_conn = db_conn_pool.connection().await.unwrap();
    let mut proof_dal = proof_db_conn.tee_proof_generation_dal();

    // there should not be any batches awaiting proof in the db yet

    let oldest_batch_number = proof_dal.get_oldest_picked_by_prover_batch().await.unwrap();
    assert!(oldest_batch_number.is_none());

    // mock SQL table with relevant information about the status of TEE proof generation

    proof_dal
        .insert_tee_proof_generation_job(batch_number, TeeType::Sgx)
        .await
        .expect("Failed to insert tee_proof_generation_job");

    // now, there should be one batch in the db awaiting proof

    let oldest_batch_number = proof_dal
        .get_oldest_picked_by_prover_batch()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(oldest_batch_number, batch_number);
}

fn create_l1_batch_header(number: u32) -> L1BatchHeader {
    L1BatchHeader::new(
        L1BatchNumber(number),
        100,
        BaseSystemContractsHashes {
            bootloader: H256::repeat_byte(1),
            default_aa: H256::repeat_byte(42),
            evm_emulator: Some(H256::repeat_byte(43)),
        },
        ProtocolVersionId::latest(),
    )
}

async fn save_default_protocol_version(pool: &ConnectionPool<Core>) {
    let mut connection = pool.connection().await.unwrap();
    connection
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();
}

async fn insert_batch_for_tee_inputs(
    pool: ConnectionPool<Core>,
    batch_number: L1BatchNumber,
    mark_as_present: bool,
) {
    let mut connection = pool.connection().await.unwrap();
    connection
        .blocks_dal()
        .insert_mock_l1_batch(&create_l1_batch_header(batch_number.0))
        .await
        .unwrap();
    connection
        .proof_generation_dal()
        .insert_proof_generation_details(batch_number)
        .await
        .unwrap();

    if mark_as_present {
        connection
            .proof_generation_dal()
            .save_vm_runner_artifacts_metadata(batch_number, "vm_run")
            .await
            .unwrap();
        connection
            .proof_generation_dal()
            .save_merkle_paths_artifacts_metadata(batch_number, "merkle_paths")
            .await
            .unwrap();
    }
}

async fn response_json(response: Response) -> serde_json::Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
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

use std::time::Duration;

use axum::{
    body::{to_bytes, Body},
    http::{self, Method, Request, StatusCode},
    response::Response,
    Router,
};
use serde_json::json;
use tower::ServiceExt;
use zksync_airbender_prover_interface::{
    api::{
        AirbenderSnarkInputsResponse, SubmitAirbenderProofRequest, SubmitAirbenderSnarkProofRequest,
    },
    outputs::L1BatchAirbenderProofForL1,
};
use zksync_config::configs::AirbenderProofDataHandlerConfig;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::MockObjectStore;
use zksync_types::{
    block::L1BatchHeader, settlement::SettlementLayer, L1BatchNumber, L2ChainId, ProtocolVersion,
    ProtocolVersionId, H256,
};

use crate::create_proof_processing_router;

fn test_config() -> AirbenderProofDataHandlerConfig {
    AirbenderProofDataHandlerConfig {
        http_port: 1337,
        first_processed_batch: L1BatchNumber(0),
        proof_generation_timeout: Duration::from_secs(600),
        snark_generation_timeout: Duration::from_secs(600),
        max_attempts: 5,
    }
}

#[tokio::test]
async fn request_airbender_proof_inputs() {
    let db_conn_pool = ConnectionPool::test_pool().await;

    let app = create_proof_processing_router(
        MockObjectStore::arc(),
        db_conn_pool.clone(),
        test_config(),
        L2ChainId::default(),
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/airbender/proof_inputs")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn request_airbender_proof_inputs_no_lock_returns_404_for_missing_batch() {
    let db_conn_pool = ConnectionPool::test_pool().await;
    let app = create_proof_processing_router(
        MockObjectStore::arc(),
        db_conn_pool,
        test_config(),
        L2ChainId::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/airbender/proof_inputs_no_lock/1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn request_airbender_proof_inputs_no_lock_returns_404_for_missing_blob_data() {
    let db_conn_pool = ConnectionPool::test_pool().await;
    save_default_protocol_version(&db_conn_pool).await;
    insert_batch_for_airbender_inputs(db_conn_pool.clone(), L1BatchNumber(1), true).await;

    let app = create_proof_processing_router(
        MockObjectStore::arc(),
        db_conn_pool,
        test_config(),
        L2ChainId::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/airbender/proof_inputs_no_lock/1")
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
        L2ChainId::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/airbender/present_batches")
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
    insert_batch_for_airbender_inputs(db_conn_pool.clone(), L1BatchNumber(1), true).await;
    insert_batch_for_airbender_inputs(db_conn_pool.clone(), L1BatchNumber(3), false).await;
    insert_batch_for_airbender_inputs(db_conn_pool.clone(), L1BatchNumber(5), true).await;

    let app = create_proof_processing_router(
        MockObjectStore::arc(),
        db_conn_pool,
        test_config(),
        L2ChainId::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/airbender/present_batches")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response_json(response).await;
    assert_eq!(body, json!({ "oldest_batch": 1, "latest_batch": 5 }));
}

// Test /airbender/submit_proofs endpoint
#[tokio::test]
async fn submit_airbender_proof() {
    let batch_number = L1BatchNumber::from(1);
    let db_conn_pool = ConnectionPool::test_pool().await;

    mock_airbender_batch_status(db_conn_pool.clone(), batch_number).await;

    let airbender_proof_request = SubmitAirbenderProofRequest {
        l1_batch_number: batch_number.0,
        prover_id: "test-prover".to_string(),
        proof: vec![0x0A, 0x0B, 0x0C, 0x0D, 0x0E],
    };
    let uri = "/airbender/submit_proofs".to_string();
    let app = create_proof_processing_router(
        MockObjectStore::arc(),
        db_conn_pool.clone(),
        test_config(),
        L2ChainId::default(),
    );

    let response = send_submit_airbender_proof_request(&app, &uri, &airbender_proof_request).await;
    assert_eq!(response.status(), StatusCode::OK);

    // there should not be any batches awaiting proof in the db anymore

    let mut proof_db_conn = db_conn_pool.connection().await.unwrap();
    let oldest_batch_number = proof_db_conn
        .airbender_proof_generation_dal()
        .get_oldest_picked_by_prover_batch()
        .await
        .unwrap();

    assert!(oldest_batch_number.is_none());

    // there should be one proof in the db now

    let proof = proof_db_conn
        .airbender_proof_generation_dal()
        .get_airbender_proof(batch_number)
        .await
        .unwrap()
        .expect("proof should exist");

    assert!(proof.proof_blob_url.is_some());
}

#[tokio::test]
async fn submit_airbender_proof_rejects_when_not_picked() {
    let batch_number = L1BatchNumber::from(1);
    let db_conn_pool = ConnectionPool::test_pool().await;

    // Do NOT insert an airbender_proof_generation_job — the batch has no row at all
    let airbender_proof_request = SubmitAirbenderProofRequest {
        l1_batch_number: batch_number.0,
        prover_id: "test-prover".to_string(),
        proof: vec![0x0A, 0x0B, 0x0C],
    };
    let app = create_proof_processing_router(
        MockObjectStore::arc(),
        db_conn_pool.clone(),
        test_config(),
        L2ChainId::default(),
    );

    let response = send_submit_airbender_proof_request(
        &app,
        "/airbender/submit_proofs",
        &airbender_proof_request,
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn snark_inputs_returns_no_content_when_empty() {
    let db_conn_pool = ConnectionPool::test_pool().await;
    let app = create_proof_processing_router(
        MockObjectStore::arc(),
        db_conn_pool,
        test_config(),
        L2ChainId::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/airbender/snark_inputs")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn snark_inputs_returns_fri_proof_and_locks_for_snark() {
    let batch_number = L1BatchNumber(1);
    let db_conn_pool = ConnectionPool::test_pool().await;
    save_default_protocol_version(&db_conn_pool).await;

    {
        let mut conn = db_conn_pool.connection().await.unwrap();
        conn.blocks_dal()
            .insert_mock_l1_batch(&create_l1_batch_header(batch_number.0))
            .await
            .unwrap();
        let mut dal = conn.airbender_proof_generation_dal();
        dal.insert_airbender_proof_generation_job(batch_number)
            .await
            .unwrap();
        dal.save_proof_artifacts_metadata(batch_number, "fri-blob-url", "fri-prover")
            .await
            .unwrap();
    }

    let fri_payload = vec![0xAA, 0xBB, 0xCC, 0xDD];
    let object_store = MockObjectStore::arc();
    object_store
        .put(
            batch_number,
            &L1BatchAirbenderProofForL1 {
                proof: fri_payload.clone(),
            },
        )
        .await
        .unwrap();

    let app = create_proof_processing_router(
        object_store,
        db_conn_pool.clone(),
        test_config(),
        L2ChainId::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/airbender/snark_inputs")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: AirbenderSnarkInputsResponse = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(parsed.l1_batch_number, batch_number.0);
    assert_eq!(parsed.fri_proof, fri_payload);

    let mut conn = db_conn_pool.connection().await.unwrap();
    let row = conn
        .airbender_proof_generation_dal()
        .get_airbender_proof(batch_number)
        .await
        .unwrap()
        .expect("row should exist");
    assert_eq!(row.status, "picked_for_snark");
}

#[tokio::test]
async fn snark_inputs_rolls_back_lock_when_fri_proof_missing_in_gcs() {
    let batch_number = L1BatchNumber(1);
    let db_conn_pool = ConnectionPool::test_pool().await;
    save_default_protocol_version(&db_conn_pool).await;

    {
        let mut conn = db_conn_pool.connection().await.unwrap();
        conn.blocks_dal()
            .insert_mock_l1_batch(&create_l1_batch_header(batch_number.0))
            .await
            .unwrap();
        let mut dal = conn.airbender_proof_generation_dal();
        dal.insert_airbender_proof_generation_job(batch_number)
            .await
            .unwrap();
        dal.save_proof_artifacts_metadata(batch_number, "fri-blob-url", "fri-prover")
            .await
            .unwrap();
    }

    // No FRI proof in the mock object store — handler should exhaust retries.
    let app = create_proof_processing_router(
        MockObjectStore::arc(),
        db_conn_pool.clone(),
        test_config(),
        L2ChainId::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/airbender/snark_inputs")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Transaction rollback should have reverted status back to `generated`,
    // not left it stuck on `picked_for_snark`.
    let mut conn = db_conn_pool.connection().await.unwrap();
    let row = conn
        .airbender_proof_generation_dal()
        .get_airbender_proof(batch_number)
        .await
        .unwrap()
        .expect("row should exist");
    assert_eq!(row.status, "generated");
}

#[tokio::test]
async fn submit_snark_proof_succeeds_when_picked_for_snark() {
    let batch_number = L1BatchNumber(1);
    let db_conn_pool = ConnectionPool::test_pool().await;

    mock_airbender_picked_for_snark(db_conn_pool.clone(), batch_number).await;

    let request = SubmitAirbenderSnarkProofRequest {
        l1_batch_number: batch_number.0,
        prover_id: "test-snark-prover".to_string(),
        snark_proof: vec![0x01, 0x02, 0x03, 0x04],
    };

    let app = create_proof_processing_router(
        MockObjectStore::arc(),
        db_conn_pool.clone(),
        test_config(),
        L2ChainId::default(),
    );

    let response =
        send_submit_snark_proof_request(&app, "/airbender/submit_snark_proofs", &request).await;
    assert_eq!(response.status(), StatusCode::OK);

    let mut conn = db_conn_pool.connection().await.unwrap();
    let snark = conn
        .airbender_proof_generation_dal()
        .get_airbender_snark_proof(batch_number)
        .await
        .unwrap()
        .expect("snark row should exist");
    assert_eq!(snark.status, "snark_generated");
    assert!(snark.snark_proof_blob_url.is_some());
}

#[tokio::test]
async fn submit_snark_proof_rejects_when_not_picked_for_snark() {
    let batch_number = L1BatchNumber(1);
    let db_conn_pool = ConnectionPool::test_pool().await;

    // No airbender row at all — submit should fail.
    let request = SubmitAirbenderSnarkProofRequest {
        l1_batch_number: batch_number.0,
        prover_id: "test-snark-prover".to_string(),
        snark_proof: vec![0x01, 0x02],
    };

    let app = create_proof_processing_router(
        MockObjectStore::arc(),
        db_conn_pool,
        test_config(),
        L2ChainId::default(),
    );

    let response =
        send_submit_snark_proof_request(&app, "/airbender/submit_snark_proofs", &request).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// Mock SQL db with information about the status of the Airbender proof generation
async fn mock_airbender_batch_status(
    db_conn_pool: ConnectionPool<zksync_dal::Core>,
    batch_number: L1BatchNumber,
) {
    let mut proof_db_conn = db_conn_pool.connection().await.unwrap();
    let mut proof_dal = proof_db_conn.airbender_proof_generation_dal();

    let oldest_batch_number = proof_dal.get_oldest_picked_by_prover_batch().await.unwrap();
    assert!(oldest_batch_number.is_none());

    proof_dal
        .insert_airbender_proof_generation_job(batch_number)
        .await
        .expect("Failed to insert airbender_proof_generation_job");

    let oldest_batch_number = proof_dal
        .get_oldest_picked_by_prover_batch()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(oldest_batch_number, batch_number);
}

// Seed a batch into `picked_for_snark` state so submit_snark_proof can succeed.
async fn mock_airbender_picked_for_snark(
    db_conn_pool: ConnectionPool<Core>,
    batch_number: L1BatchNumber,
) {
    let mut conn = db_conn_pool.connection().await.unwrap();
    let mut dal = conn.airbender_proof_generation_dal();

    dal.insert_airbender_proof_generation_job(batch_number)
        .await
        .expect("Failed to insert airbender_proof_generation_job");
    dal.save_proof_artifacts_metadata(batch_number, "fri-blob-url", "fri-prover")
        .await
        .expect("Failed to save FRI proof artifacts");

    let locked = dal
        .lock_batch_for_snark(Duration::from_secs(600), L1BatchNumber(0))
        .await
        .expect("Failed to lock batch for SNARK")
        .expect("Expected the seeded batch to be lockable for SNARK");
    assert_eq!(locked.l1_batch_number, batch_number);
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
        SettlementLayer::for_tests(),
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

async fn insert_batch_for_airbender_inputs(
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
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("failed to read response body");
    serde_json::from_slice(&body).unwrap_or_else(|err| {
        panic!(
            "failed to parse response as JSON: {err}\nbody: {}",
            String::from_utf8_lossy(&body)
        )
    })
}

async fn send_submit_airbender_proof_request(
    app: &Router,
    uri: &str,
    airbender_proof_request: &SubmitAirbenderProofRequest,
) -> Response {
    let req_body = Body::from(serde_json::to_vec(airbender_proof_request).unwrap());
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

async fn send_submit_snark_proof_request(
    app: &Router,
    uri: &str,
    request: &SubmitAirbenderSnarkProofRequest,
) -> Response {
    let req_body = Body::from(serde_json::to_vec(request).unwrap());
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

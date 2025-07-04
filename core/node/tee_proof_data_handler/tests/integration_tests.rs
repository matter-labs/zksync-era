use std::time::Duration;

use axum::{
    body::Body,
    http::{self, Method, Request, StatusCode},
    response::Response,
    Router,
};
use serde_json::json;
use tower::ServiceExt;
use zksync_config::configs::TeeProofDataHandlerConfig;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{ConnectionPool, CoreDal};
use zksync_multivm::zk_evm_latest::ethereum_types::H256;
use zksync_object_store::MockObjectStore;
use zksync_tee_prover_interface::api::{RegisterTeeAttestationRequest, SubmitTeeProofRequest};
use zksync_types::{
    block::L1BatchHeader, tee_types::TeeType, L1BatchNumber, L2ChainId, ProtocolVersion,
    ProtocolVersionId,
};

use zksync_tee_proof_data_handler::create_proof_processing_router;

fn test_config() -> TeeProofDataHandlerConfig {
    TeeProofDataHandlerConfig {
        http_port: 0,
        first_processed_batch: L1BatchNumber(0),
        proof_generation_timeout_in_secs: Duration::from_secs(600),
        batch_permanently_ignored_timeout_in_hours: Duration::from_secs(10 * 24 * 3_600),
        dcap_collateral_refresh_in_hours: Duration::from_secs(24 * 60 * 60),
    }
}

#[tokio::test]
async fn request_tee_proof_inputs() {
    let db_conn_pool = ConnectionPool::test_pool().await;

    let app = create_proof_processing_router(
        MockObjectStore::arc(),
        db_conn_pool.clone(),
        test_config(),
        L2ChainId::default(),
    );
    let test_cases = vec![
        (json!({ "tee_type": "sgx" }), StatusCode::NO_CONTENT),
        (json!({ "tee_type": "tdx" }), StatusCode::NO_CONTENT),
        (
            json!({ "tee_type": "Sgx" }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({ "tee_type": "Tdx" }),
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

// Test /tee/submit_proofs endpoint using a mocked TEE proof and verify response and db state
#[tokio::test]
async fn submit_tee_proof() {
    let batch_number = L1BatchNumber::from(650000);
    let db_conn_pool = ConnectionPool::test_pool().await;

    let tee_proof_request_str = r#"{
        "signature": "26532cd6d22957f2e4ba83765cf3a82e9632279e5b3d5153a8a699eb42f0108713e899071531e7685ef8d099493ce613f9f3fcc84b159b5302a3c61507d688a11b",
        "pubkey": "726cfd7c264de899cfcd9e1a0d05a83f23321c9b",
        "proof": "8ac786c8f405e392baa59432c5fee7729e896a1220fbba788d9cffe953db8649",
        "tee_type": "tdx"
    }"#;
    let tee_proof_request =
        serde_json::from_str::<SubmitTeeProofRequest>(tee_proof_request_str).unwrap();

    mock_tee_batch_status(
        db_conn_pool.clone(),
        batch_number,
        H256::from_slice(tee_proof_request.0.proof.as_slice()),
    )
    .await;

    let uri = format!("/tee/submit_proofs/{}", batch_number.0);
    let app = create_proof_processing_router(
        MockObjectStore::arc(),
        db_conn_pool.clone(),
        test_config(),
        L2ChainId::default(),
    );

    // this should fail because we haven't saved the attestation for the pubkey yet

    let response = send_submit_tee_proof_request(&app, &uri, &tee_proof_request).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    // save the attestation for the pubkey

    let attestation = include_bytes!("data/650000_quote.bin");
    let register_tee_attestation_request = RegisterTeeAttestationRequest {
        attestation: attestation.to_vec(),
        pubkey: tee_proof_request.0.pubkey.clone(),
    };
    let response = send_submit_attestation_request(&app, &register_tee_attestation_request).await;
    let response_status = response.status();
    if response_status != StatusCode::OK {
        let response_body = axum::body::to_bytes(response.into_body(), 65000)
            .await
            .unwrap();
        let response_text = String::from_utf8_lossy(&response_body);
        panic!(
            "Response status: {}, Response body: {}",
            response_status, response_text
        );
    }

    // resend the same request; this time, it should be successful

    let response = send_submit_tee_proof_request(&app, &uri, &tee_proof_request).await;
    if response_status != StatusCode::OK {
        let response_body = axum::body::to_bytes(response.into_body(), 65000)
            .await
            .unwrap();
        let response_text = String::from_utf8_lossy(&response_body);
        panic!(
            "Response status: {}, Response body: {}",
            response_status, response_text
        );
    }

    // there should not be any batches awaiting proof in the db anymore

    let mut proof_db_conn = db_conn_pool.connection().await.unwrap();
    let oldest_batch_number = proof_db_conn
        .tee_proof_generation_dal()
        .get_oldest_picked_by_prover_batch()
        .await
        .unwrap();

    assert!(oldest_batch_number.is_none());

    // there should be one TDX proof in the db now

    let proofs = proof_db_conn
        .tee_proof_generation_dal()
        .get_tee_proofs(batch_number, Some(TeeType::Tdx))
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

    let attestations = proof_db_conn
        .tee_proof_generation_dal()
        .get_pending_attestations_for_eth_tx()
        .await
        .unwrap();
    assert_eq!(attestations.len(), 1);
}

pub(crate) fn create_l1_batch_header(number: u32) -> L1BatchHeader {
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

// Mock SQL db with information about the status of the TEE proof generation
async fn mock_tee_batch_status(
    db_conn_pool: ConnectionPool<zksync_dal::Core>,
    batch_number: L1BatchNumber,
    root_hash: H256,
) {
    let mut proof_db_conn = db_conn_pool.connection().await.unwrap();

    let mut protocol_versions_dal = proof_db_conn.protocol_versions_dal();
    protocol_versions_dal
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();

    let mut blocks_dal = proof_db_conn.blocks_dal();

    blocks_dal
        .insert_mock_l1_batch(&create_l1_batch_header(batch_number.0))
        .await
        .unwrap();
    blocks_dal
        .set_l1_batch_hash(batch_number, root_hash)
        .await
        .unwrap();

    let mut proof_dal = proof_db_conn.tee_proof_generation_dal();

    // there should not be any batches awaiting proof in the db yet

    let oldest_batch_number = proof_dal.get_oldest_picked_by_prover_batch().await.unwrap();
    assert!(oldest_batch_number.is_none());

    // mock SQL table with relevant information about the status of TEE proof generation

    proof_dal
        .insert_tee_proof_generation_job(batch_number, TeeType::Tdx)
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

async fn send_submit_attestation_request(
    app: &Router,
    register_tee_attestation_request: &RegisterTeeAttestationRequest,
) -> Response {
    let req_body = Body::from(serde_json::to_vec(register_tee_attestation_request).unwrap());
    app.clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/tee/register_attestation")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(req_body)
                .unwrap(),
        )
        .await
        .unwrap()
}

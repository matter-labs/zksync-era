use std::env;

use axum::{
    body::Body,
    http::{self, Method, Request, StatusCode},
    response::Response,
    Router,
};
use serde_json::json;
use tokio::time::{sleep, Duration};
use tower::ServiceExt;
use zksync_config::configs::ProofDataHandlerConfig;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{ConnectionPool, CoreDal};
use zksync_object_store::MockObjectStore;
use zksync_prover_interface::api::SubmitTeeProofRequest;
use zksync_types::{
    block::L1BatchHeader,
    commitment::{L1BatchCommitmentArtifacts, L1BatchCommitmentMode},
    protocol_version::ProtocolSemanticVersion,
    tee_types::TeeType,
    L1BatchNumber, ProtocolVersionId, H256,
};

use crate::create_proof_processing_router;

#[tokio::test]
async fn request_tee_proof_inputs() {
    let batch_number = L1BatchNumber::from(1);
    let db_conn_pool = ConnectionPool::test_pool().await;
    println!("DATABASE_URL: {:?}", db_conn_pool);

    let app = create_proof_processing_router(
        MockObjectStore::arc(),
        db_conn_pool.clone(),
        ProofDataHandlerConfig {
            http_port: 1337,
            proof_generation_timeout_in_secs: 10,
            tee_support: true,
        },
        L1BatchCommitmentMode::Rollup,
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

    mock_l1_batch(db_conn_pool.clone(), batch_number).await;

    let test_cases = vec![
        (json!({ "tee_type": "sgx" }), StatusCode::OK),
        (
            json!({ "tee_type": "Sgx" }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ];

    sleep(Duration::from_secs(1000)).await;

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
    let batch_number = L1BatchNumber::from(1);
    let db_conn_pool = ConnectionPool::test_pool().await;

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

async fn mock_l1_batch(
    db_conn_pool: ConnectionPool<zksync_dal::Core>,
    batch_number: L1BatchNumber,
) {
    let mut proof_db_conn = db_conn_pool.connection().await.unwrap();

    // Common values
    let protocol_version_id = ProtocolVersionId::latest();
    let protocol_version = ProtocolSemanticVersion {
        minor: protocol_version_id,
        patch: 0.into(),
    };
    let base_system_contracts_hashes = BaseSystemContractsHashes {
        bootloader: H256::repeat_byte(1),
        default_aa: H256::repeat_byte(42),
    };

    // Save protocol version
    proof_db_conn
        .protocol_versions_dal()
        .save_protocol_version(
            protocol_version,
            0,
            Default::default(),
            base_system_contracts_hashes,
            None,
        )
        .await
        .unwrap();

    // Insert mock L1 batch
    let header = L1BatchHeader::new(
        batch_number,
        100,
        base_system_contracts_hashes,
        protocol_version_id,
    );
    assert!(proof_db_conn
        .blocks_dal()
        .insert_mock_l1_batch(&header)
        .await
        .is_ok());

    // Save L1 batch commitment artifacts and set hash
    let hash = H256::repeat_byte(1);
    proof_db_conn
        .blocks_dal()
        .save_l1_batch_commitment_artifacts(batch_number, &L1BatchCommitmentArtifacts::default())
        .await
        .unwrap();
    proof_db_conn
        .blocks_dal()
        .set_l1_batch_hash(batch_number, hash)
        .await
        .unwrap();

    // Insert proof generation details
    proof_db_conn
        .proof_generation_dal()
        .insert_proof_generation_details(batch_number)
        .await
        .unwrap();

    proof_db_conn
        .proof_generation_dal()
        .save_vm_runner_artifacts_metadata(batch_number, "vm_run_data_blob_url")
        .await
        .unwrap();

    proof_db_conn
        .proof_generation_dal()
        .save_merkle_paths_artifacts_metadata(batch_number, "proof_gen_data_blob_url")
        .await
        .unwrap();
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

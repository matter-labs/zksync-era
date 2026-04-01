use std::time::Duration;

use axum::{
    body::Body,
    http::{self, Method, Request, StatusCode},
    response::Response,
    Router,
};
use tower::ServiceExt;
use zksync_airbender_prover_interface::api::SubmitAirbenderProofRequest;
use zksync_config::configs::AirbenderProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, CoreDal};
use zksync_object_store::MockObjectStore;
use zksync_types::{L1BatchNumber, L2ChainId};

use crate::create_proof_processing_router;

fn test_config() -> AirbenderProofDataHandlerConfig {
    AirbenderProofDataHandlerConfig {
        http_port: 1337,
        first_processed_batch: L1BatchNumber(0),
        proof_generation_timeout: Duration::from_secs(600),
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

// Test /airbender/submit_proofs endpoint
#[tokio::test]
async fn submit_airbender_proof() {
    let batch_number = L1BatchNumber::from(1);
    let db_conn_pool = ConnectionPool::test_pool().await;

    mock_airbender_batch_status(db_conn_pool.clone(), batch_number).await;

    let airbender_proof_request = SubmitAirbenderProofRequest {
        proof: vec![0x0A, 0x0B, 0x0C, 0x0D, 0x0E],
    };
    let uri = format!("/airbender/submit_proofs/{}", batch_number.0);
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

    let proofs = proof_db_conn
        .airbender_proof_generation_dal()
        .get_airbender_proofs(batch_number)
        .await
        .unwrap();

    assert_eq!(proofs.len(), 1);

    let proof = &proofs[0];
    assert!(proof.proof_blob_url.is_some());
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

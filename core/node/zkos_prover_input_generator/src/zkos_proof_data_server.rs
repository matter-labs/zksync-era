use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router, serve,
};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use axum::extract::DefaultBodyLimit;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::{error, info};
use tracing_subscriber;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::L2BlockNumber;

#[derive(Debug, Serialize, Deserialize)]
struct NextProverJobPayload {
    block_number: u32,
    prover_input: String, // base64-encoded
}

#[derive(Debug, Serialize, Deserialize)]
struct ProofPayload {
    block_number: u32,
    proof: String,
}

/// Handler to fetch the next FRI block to prove
async fn pick_fri_job(
    State(pool): State<Arc<ConnectionPool<Core>>>,
) -> Response {
    let mut conn = pool
        .connection_tagged("zkos_proof_data_server")
        .await
        .expect("Failed to get DB connection");

    info!("Fetching next FRI block to prove");
    let response = match conn.zkos_prover_dal()
        .pick_next_fri_proof(Duration::from_secs(60), "unknown")
        .await
    {
        Ok(Some((block_number, data))) => {
            info!("Picked FRI block to prove: {}", block_number);
            let encoded = base64::encode(&data);
            let resp = NextProverJobPayload {
                block_number: block_number.0,
                prover_input: encoded,
            };
            Json(resp).into_response()
        }
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => {
            error!("Error fetching next FRI proof job: {}", err);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    };

    response
}

/// Handler to submit an FRI proof
async fn submit_fri_proof(
    State(pool): State<Arc<ConnectionPool<Core>>>,
    Json(payload): Json<ProofPayload>,
) -> Response {
    let mut conn = pool
        .connection_tagged("zkos_proof_data_server")
        .await
        .expect("Failed to get DB connection");

    let proof_bytes = match base64::decode(&payload.proof) {
        Ok(b) => b,
        Err(err) => {
            error!("Invalid base64 FRI proof: {}", err);
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    let block_number = L2BlockNumber(payload.block_number);
    info!("Submitting FRI proof for block {}", block_number);
    match conn.zkos_prover_dal()
        .save_fri_proof(block_number, proof_bytes)
        .await
    {
        Ok(()) => ().into_response(),
        Err(err) => {
            error!("Error saving FRI proof: {}", err);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Handler to fetch the next SNARK block to prove
async fn pick_snark_job(
    State(pool): State<Arc<ConnectionPool<Core>>>,
) -> Response {
    let mut conn = pool
        .connection_tagged("zkos_proof_data_server")
        .await
        .expect("Failed to get DB connection");

    info!("Fetching next SNARK block to prove");
    let response = match conn.zkos_prover_dal()
        .pick_next_snark_proof(Duration::from_secs(3600), "unknown")
        .await
    {
        Ok(Some((block_number, data))) => {
            info!("Picked SNARK block to prove: {}", block_number);
            let encoded = base64::encode(&data);
            let resp = NextProverJobPayload {
                block_number: block_number.0,
                prover_input: encoded,
            };
            Json(resp).into_response()
        }
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => {
            error!("Error fetching next SNARK proof job: {}", err);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    };

    response
}

/// Handler to submit a SNARK proof (stub)
async fn submit_snark_proof(
    State(pool): State<Arc<ConnectionPool<Core>>>,
    Json(payload): Json<ProofPayload>,
) -> Response {
    // TODO: implement save_snark_proof in DAL
    let mut conn = pool
        .connection_tagged("zkos_proof_data_server")
        .await
        .expect("Failed to get DB connection");

    let proof_bytes = match base64::decode(&payload.proof) {
        Ok(b) => b,
        Err(err) => {
            error!("Invalid base64 SNARK proof: {}", err);
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    let block_number = L2BlockNumber(payload.block_number);
    info!("Submitting SNARK proof for block {}", block_number);
    match conn.zkos_prover_dal()
        .save_snark_proof(block_number, proof_bytes)
        .await
    {
        Ok(()) => ().into_response(),
        Err(err) => {
            error!("Error saving SNARK proof: {}", err);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Create and run the HTTP server
pub async fn run(pool: ConnectionPool<Core>) -> anyhow::Result<()> {
    let shared_pool = Arc::new(pool);

    let app = Router::new()
        // FRI proof routes
        .route(
            "/prover-jobs/FRI/pick",
            post(pick_fri_job),
        )
        .route(
            "/prover-jobs/FRI/submit",
            post(submit_fri_proof),
        )
        // SNARK proof routes
        .route(
            "/prover-jobs/SNARK/pick",
            post(pick_snark_job),
        )
        .route(
            "/prover-jobs/SNARK/submit",
            post(submit_snark_proof),
        )
        .layer(DefaultBodyLimit::disable())
        .with_state(shared_pool);

    let bind_address = SocketAddr::from(([0, 0, 0, 0], 3124));
    info!("Starting proof data handler server on {}", bind_address);

    let listener = TcpListener::bind(bind_address)
        .await
        .expect("Failed binding proof data handler server");

    serve(listener, app).await?;
    Ok(())
}

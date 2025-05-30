use axum::{extract::State, http::StatusCode, response::{IntoResponse, Response}, routing::{get, post}, Json, Router, serve};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};
use std::time::Duration;
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

/// Handler to fetch the next block to prove

/// Handler to fetch the next block to prove
async fn pick_next_prover_job(
    State(pool): State<Arc<ConnectionPool<Core>>>,
) -> Response {
    let mut conn = pool
        .connection_tagged("zkos_proof_data_server")
        .await
        .expect("Failed to get DB connection");

    tracing::info!("Fetching next block to prove");
    // config
    let response: Response = match conn.zkos_prover_dal().pick_next_proof(Duration::from_secs(60), "unknown").await {
        Ok(Some((block_number, data))) => {
            tracing::info!("Picked and returned block to prove: {block_number}");
            let encoded = base64::encode(&data);
            let resp = NextProverJobPayload { block_number: block_number.0, prover_input: encoded };
            Json(resp).into_response()
        }
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => {
            error!("Error fetching next block: {err}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    };

    response
}

/// Handler to submit a proof
async fn submit_proof(
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
            error!("Invalid base64 proof: {err}");
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    let block_number = L2BlockNumber(payload.block_number);
    tracing::info!("Submited proof for block {block_number}");
    let response: Response = match conn.zkos_prover_dal().save_proof(block_number, proof_bytes).await {
        Ok(()) => {
            ().into_response()
        }
        Err(err) => {
            error!("Error processing proof: {err}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    };

    response
}

/// Create and run the HTTP server
pub async fn run(pool: ConnectionPool<Core>) -> anyhow::Result<()> {
    let shared_pool = Arc::new(pool);

    let app = Router::new()
        .route("/pick-prover-job/FRI", get(pick_next_prover_job))
        .route("/submit-proof/FRI", post(submit_proof))
        .with_state(shared_pool);

    let bind_address = SocketAddr::from(([0, 0, 0, 0], 3124));
    info!("Starting proof data handler server on {bind_address}");

    let listener = TcpListener::bind(bind_address)
        .await
        .expect("Failed binding proof data handler server");

    // Serve the Axum app over the TCP listener
    serve(listener, app).await?;
    Ok(())
}

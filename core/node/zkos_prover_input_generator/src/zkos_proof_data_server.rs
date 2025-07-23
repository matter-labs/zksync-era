use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    serve, Json, Router,
};
use execution_utils::ProgramProof;
use proof_cache::client::ProofCacheClient;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tracing::{error, info};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_types::{commitment::L1BatchWithMetadata, L1BatchNumber, L2BlockNumber};

use crate::{metrics::PROVER_STATE_METRICS, proof_verifier::verify_fri_proof};

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
#[derive(Debug, Serialize, Deserialize)]
struct AvailableProofsPayload {
    block_number: u32,
    available_proofs: Vec<String>,
}
/// Handler to fetch the next FRI block to prove
async fn pick_fri_job(State(pool): State<Arc<ConnectionPool<Core>>>) -> Response {
    let mut conn = pool
        .connection_tagged("zkos_proof_data_server")
        .await
        .expect("Failed to get DB connection");

    tracing::trace!("Fetching next FRI block to prove");
    let response = match conn
        .zkos_prover_dal()
        .pick_next_fri_proof(Duration::from_secs(60), 5, "unknown")
        .await
    {
        Ok(Some((block_number, data))) => {
            info!("Picked FRI block to prove: {}", block_number);

            PROVER_STATE_METRICS.fri_queue.set(block_number.0 as usize);
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
) -> Result<Response, (StatusCode, String)> {
    let mut conn = pool
        .connection_tagged("zkos_proof_data_server")
        .await
        .expect("Failed to get DB connection");

    info!("Received FRI proof for block {}", payload.block_number);
    PROVER_STATE_METRICS
        .latest_submitted_fri_proof
        .set(payload.block_number as usize);

    let proof_bytes = base64::decode(&payload.proof).map_err(|err| {
        error!("Invalid base64 FRI proof: {err}");
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid base64 FRI proof: {err}"),
        )
    })?;

    let current_l1_batch = load_batch_metadata(&mut conn, L1BatchNumber(payload.block_number))
        .await
        .map_err(|err| {
            error!("Error loading current L1 batch metadata: {}", err);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Error loading current L1 batch metadata: {}", err),
            )
        })?;
    let prev_l1_batch = load_batch_metadata(&mut conn, L1BatchNumber(payload.block_number - 1))
        .await
        .map_err(|err| {
            error!("Error loading previous L1 batch metadata: {}", err);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Error loading previous L1 batch metadata: {}", err),
            )
        })?;

    let program_proof = bincode::deserialize::<ProgramProof>(&proof_bytes).map_err(|err| {
        error!("Unable to deserialize FRI proof: {}", err);
        (
            StatusCode::BAD_REQUEST,
            format!("Unable to deserialize FRI proof: {}", err),
        )
    })?;

    verify_fri_proof(prev_l1_batch, current_l1_batch, program_proof.clone()).map_err(|err| {
        error!("FRI proof verification failed: {}", err);
        (
            StatusCode::BAD_REQUEST,
            format!("FRI proof verification failed: {}", err),
        )
    })?;

    // TODO: inject client in State and reuse it
    // TODO: address needs to be configurable, not hardcoded
    let proof_cache_client = ProofCacheClient::new("http://localhost:3815".to_string()).map_err(|err| {
        error!("Failed to create proof cache client: {:?}", err);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create proof cache client: {}", err),
        )
    })?;
    proof_cache_client.put_fri(&payload.block_number.to_string(), program_proof)
        .await
        .map_err(|err| {
            error!("Failed to submit FRI proof to cache: {:?}", err);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to submit FRI proof to cache: {}", err),
            )
        })?;
    conn.zkos_prover_dal()
        .save_fri_proof(L2BlockNumber(payload.block_number), proof_bytes)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Error saving FRI proof: {}", err),
            )
        })?;
    Ok((
        StatusCode::NO_CONTENT,
        "FRI proof submitted successfully".to_string(),
    )
        .into_response())
}

/// Handler to fetch the next SNARK block to prove
async fn pick_snark_job(State(pool): State<Arc<ConnectionPool<Core>>>) -> Response {
    let mut conn = pool
        .connection_tagged("zkos_proof_data_server")
        .await
        .expect("Failed to get DB connection");

    tracing::trace!("Fetching next SNARK block to prove");
    let response = match conn
        .zkos_prover_dal()
        .pick_next_snark_proof(Duration::from_secs(3600), "unknown")
        .await
    {
        Ok(Some((block_number, data))) => {
            PROVER_STATE_METRICS
                .snark_queue
                .set(block_number.0 as usize);
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
    PROVER_STATE_METRICS
        .latest_submitted_snark_proof
        .set(payload.block_number as usize);
    match conn
        .zkos_prover_dal()
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

/// Handler to list blocks with available proofs
async fn list_available_proofs(State(pool): State<Arc<ConnectionPool<Core>>>) -> Response {
    let mut conn = pool
        .connection_tagged("zkos_proof_data_server")
        .await
        .expect("Failed to get DB connection");

    info!("Fetching available proofs per block");
    // TODO: implement `list_available_proofs` in DAL. Expected signature:
    //   async fn list_available_proofs(&mut self) -> anyhow::Result<Vec<(L2BlockNumber, Vec<String>)>>
    match conn.zkos_prover_dal().list_available_proofs().await {
        Ok(rows) => {
            let payload: Vec<AvailableProofsPayload> = rows
                .into_iter()
                .map(|(block_number, proofs)| AvailableProofsPayload {
                    block_number: block_number.0,
                    available_proofs: proofs,
                })
                .collect();
            Json(payload).into_response()
        }
        Err(err) => {
            error!("Error fetching available proofs: {}", err);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// NEW: Handler to fetch a specific proof by type and block number
async fn get_proof(
    Path((proof_type, block_number)): Path<(String, u32)>,
    State(pool): State<Arc<ConnectionPool<Core>>>,
) -> Response {
    let mut conn = pool
        .connection_tagged("zkos_proof_data_server")
        .await
        .expect("Failed to get DB connection");

    let block_number_l2 = L2BlockNumber(block_number);
    match proof_type.as_str() {
        "FRI" => {
            info!("Fetching FRI proof for block {}", block_number_l2);
            // TODO: implement `get_fri_proof` in DAL
            match conn.zkos_prover_dal().get_fri_proof(block_number_l2).await {
                Ok(Some(bytes)) => {
                    let resp = ProofPayload {
                        block_number,
                        proof: base64::encode(&bytes),
                    };
                    Json(resp).into_response()
                }
                Ok(None) => StatusCode::NO_CONTENT.into_response(),
                Err(err) => {
                    error!("Error fetching FRI proof: {}", err);
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            }
        }
        "SNARK" => {
            info!("Fetching SNARK proof for block {}", block_number_l2);
            // TODO: implement `get_snark_proof` in DAL
            match conn
                .zkos_prover_dal()
                .get_snark_proof(block_number_l2)
                .await
            {
                Ok(Some(bytes)) => {
                    let resp = ProofPayload {
                        block_number,
                        proof: base64::encode(&bytes),
                    };
                    Json(resp).into_response()
                }
                Ok(None) => StatusCode::NO_CONTENT.into_response(),
                Err(err) => {
                    error!("Error fetching SNARK proof: {}", err);
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            }
        }
        other => {
            error!("Unknown proof type requested: {}", other);
            StatusCode::BAD_REQUEST.into_response()
        }
    }
}

/// Create and run the HTTP server
pub async fn run(pool: ConnectionPool<Core>) -> anyhow::Result<()> {
    let shared_pool = Arc::new(pool);

    let app = Router::new()
        // FRI proof routes
        .route("/prover-jobs/FRI/pick", post(pick_fri_job))
        .route("/prover-jobs/FRI/submit", post(submit_fri_proof))
        // SNARK proof routes
        .route("/prover-jobs/SNARK/pick", post(pick_snark_job))
        .route("/prover-jobs/SNARK/submit", post(submit_snark_proof))
        .route("/prover-jobs/available", get(list_available_proofs))
        .route("/prover-jobs/:proof_type/:block_number", get(get_proof))
        .layer(axum::extract::DefaultBodyLimit::disable())
        .with_state(shared_pool);

    let bind_address = SocketAddr::from(([0, 0, 0, 0], 3124));
    info!("Starting proof data handler server on {}", bind_address);

    let listener = TcpListener::bind(bind_address)
        .await
        .expect("Failed binding proof data handler server");

    serve(listener, app).await?;
    Ok(())
}

async fn load_batch_metadata(
    connection: &mut Connection<'_, Core>,
    number: L1BatchNumber,
) -> Result<L1BatchWithMetadata, String> {
    connection
        .blocks_dal()
        .get_l1_batch_metadata(number)
        .await
        .map_err(|e| e.to_string())
        .and_then(|opt| opt.ok_or_else(|| format!("No metadata found for batch {}", number)))
}

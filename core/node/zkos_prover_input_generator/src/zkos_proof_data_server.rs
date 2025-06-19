use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    serve, Json, Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use execution_utils::ProgramProof;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::net::TcpListener;
use tracing::{debug, error, info};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_object_store::{Bucket, ObjectStore};
use zksync_types::{commitment::L1BatchWithMetadata, L1BatchNumber, L2BlockNumber};

use crate::proof_verifier::verify_fri_proof;

/// Upload proof to S3 and return error response if it fails
async fn upload_proof_to_s3(
    object_store: &Arc<dyn ObjectStore>,
    proof_type: &str,
    block_number: L2BlockNumber,
    proof_bytes: &[u8],
) -> Result<(), Response> {
    let s3_key = format!("{}_proof_{}", proof_type.to_lowercase(), block_number.0);
    info!("Uploading {} proof to S3 with key: {}", proof_type, s3_key);

    match object_store
        .put_raw(Bucket::ZkOSProofs, &s3_key, proof_bytes.to_vec())
        .await
    {
        Ok(_) => {
            info!(
                "Successfully uploaded {} proof to S3 for block {}",
                proof_type, block_number
            );
            Ok(())
        }
        Err(err) => {
            error!(
                "Failed to upload {} proof to S3 for block {}: {}",
                proof_type, block_number, err
            );
            // Return error to client and don't save to database
            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                format!("Failed to store proof: {}", err),
            )
                .into_response())
        }
    }
}

struct AppState {
    pool: Arc<ConnectionPool<Core>>,
    object_store: Option<Arc<dyn ObjectStore>>,
}

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
async fn pick_fri_job(State(state): State<Arc<AppState>>) -> Response {
    let mut conn = state
        .pool
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
            let encoded = BASE64.encode(&data);
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
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ProofPayload>,
) -> Response {
    let mut conn = state
        .pool
        .connection_tagged("zkos_proof_data_server")
        .await
        .expect("Failed to get DB connection");

    let proof_bytes = match BASE64.decode(&payload.proof) {
        Ok(b) => b,
        Err(err) => {
            error!("Invalid base64 FRI proof: {}", err);
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    let block_number = L2BlockNumber(payload.block_number);
    info!("Received FRI proof submission for block {}", block_number);
    debug!("FRI proof size: {} bytes", proof_bytes.len());

    // Load batch metadata for verification
    let current_l1_batch =
        match load_batch_metadata(&mut conn, L1BatchNumber(payload.block_number)).await {
            Ok(batch) => batch,
            Err(err) => {
                error!("Error loading current L1 batch metadata: {}", err);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

    let prev_l1_batch =
        match load_batch_metadata(&mut conn, L1BatchNumber(payload.block_number - 1)).await {
            Ok(batch) => batch,
            Err(err) => {
                error!("Error loading previous L1 batch metadata: {}", err);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

    // Deserialize and verify the proof
    let program_proof = match bincode::deserialize::<ProgramProof>(&proof_bytes) {
        Ok(proof) => proof,
        Err(err) => {
            error!("Unable to deserialize FRI proof: {}", err);
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    if let Err(err) = verify_fri_proof(prev_l1_batch, current_l1_batch, program_proof) {
        error!("FRI proof verification failed: {}", err);
        return StatusCode::BAD_REQUEST.into_response();
    }

    // Upload to S3 if object store is configured
    if let Some(object_store) = &state.object_store {
        upload_proof_to_s3(object_store, "FRI", block_number, &proof_bytes).await?;
    } else {
        debug!("Object store not configured, skipping S3 upload for FRI proof");
    }

    // Save to database only after successful S3 upload
    info!("Saving FRI proof to database for block {}", block_number);
    match conn
        .zkos_prover_dal()
        .save_fri_proof(block_number, proof_bytes)
        .await
    {
        Ok(()) => {
            info!("Successfully stored FRI proof for block {}", block_number);
            StatusCode::OK.into_response()
        }
        Err(err) => {
            error!(
                "Failed to save FRI proof to database for block {}: {}",
                block_number, err
            );
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Handler to fetch the next SNARK block to prove
async fn pick_snark_job(State(state): State<Arc<AppState>>) -> Response {
    let mut conn = state
        .pool
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
            info!("Picked SNARK block to prove: {}", block_number);
            let encoded = BASE64.encode(&data);
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
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ProofPayload>,
) -> Response {
    // TODO: implement save_snark_proof in DAL
    let mut conn = state
        .pool
        .connection_tagged("zkos_proof_data_server")
        .await
        .expect("Failed to get DB connection");

    let proof_bytes = match BASE64.decode(&payload.proof) {
        Ok(b) => b,
        Err(err) => {
            error!("Invalid base64 SNARK proof: {}", err);
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    let block_number = L2BlockNumber(payload.block_number);
    info!("Received SNARK proof submission for block {}", block_number);
    debug!("SNARK proof size: {} bytes", proof_bytes.len());

    // Upload to S3 if object store is configured
    if let Some(object_store) = &state.object_store {
        upload_proof_to_s3(object_store, "SNARK", block_number, &proof_bytes).await?;
    } else {
        debug!("Object store not configured, skipping S3 upload for SNARK proof");
    }

    // Save to database only after successful S3 upload
    info!("Saving SNARK proof to database for block {}", block_number);
    match conn
        .zkos_prover_dal()
        .save_snark_proof(block_number, proof_bytes)
        .await
    {
        Ok(()) => {
            info!("Successfully stored SNARK proof for block {}", block_number);
            StatusCode::OK.into_response()
        }
        Err(err) => {
            error!(
                "Failed to save SNARK proof to database for block {}: {}",
                block_number, err
            );
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Handler to list blocks with available proofs
async fn list_available_proofs(State(state): State<Arc<AppState>>) -> Response {
    let mut conn = state
        .pool
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
    State(state): State<Arc<AppState>>,
) -> Response {
    let mut conn = state
        .pool
        .connection_tagged("zkos_proof_data_server")
        .await
        .expect("Failed to get DB connection");

    let block_number_l2 = L2BlockNumber(block_number);

    let proof_result = match proof_type.as_str() {
        "FRI" => {
            info!("Fetching FRI proof for block {}", block_number_l2);
            conn.zkos_prover_dal().get_fri_proof(block_number_l2).await
        }
        "SNARK" => {
            info!("Fetching SNARK proof for block {}", block_number_l2);
            conn.zkos_prover_dal()
                .get_snark_proof(block_number_l2)
                .await
        }
        other => {
            error!("Unknown proof type requested: {}", other);
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    match proof_result {
        Ok(Some(bytes)) => {
            let resp = ProofPayload {
                block_number,
                proof: BASE64.encode(&bytes),
            };
            Json(resp).into_response()
        }
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => {
            error!("Error fetching {} proof: {}", proof_type, err);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Create and run the HTTP server
pub async fn run(
    pool: ConnectionPool<Core>,
    object_store: Option<Arc<dyn ObjectStore>>,
) -> anyhow::Result<()> {
    let state = Arc::new(AppState {
        pool: Arc::new(pool),
        object_store,
    });

    let app = Router::new()
        // FRI proof routes
        .route("/prover-jobs/FRI/pick", post(pick_fri_job))
        .route("/prover-jobs/FRI/submit", post(submit_fri_proof))
        // SNARK proof routes
        .route("/prover-jobs/SNARK/pick", post(pick_snark_job))
        .route("/prover-jobs/SNARK/submit", post(submit_snark_proof))
        .route("/prover-jobs/available", get(list_available_proofs))
        .route("/prover-jobs/:proof_type/:block_number", get(get_proof))
        .layer(DefaultBodyLimit::disable())
        .with_state(state);

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

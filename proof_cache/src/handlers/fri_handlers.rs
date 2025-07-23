use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use zksync_airbender_execution_utils::ProgramProof;

pub async fn get(Path(key): Path<String>, State(state): State<AppState>) -> Result<Json<ProgramProof>, StatusCode> {
    tracing::info!("fri_proofs/get({key})");
    state.get_fri_proof(&key).await.map(Json).ok_or(StatusCode::NOT_FOUND)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PutFriProof {
    pub block_number: String,
    pub proof: ProgramProof,
}

pub async fn put(State(state): State<AppState>, Json(proof_payload): Json<PutFriProof>) -> Result<StatusCode, StatusCode> {
    tracing::info!("fri_proofs/put({}, <proof>)", proof_payload.block_number);
    state.put_fri_proof(proof_payload.block_number, proof_payload.proof).await;
    Ok(StatusCode::CREATED)
}
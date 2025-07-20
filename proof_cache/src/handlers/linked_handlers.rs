use crate::state::{AppState, LinkedProof};
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use zksync_airbender_execution_utils::ProgramProof;

pub async fn get(State(state): State<AppState>) -> Result<Json<LinkedProof>, StatusCode> {
    tracing::info!("linked/get()");
    match state.get_linked_proof().await {
        Some(linked_proof) => {
            tracing::info!("LinkedProof found: {:?}", linked_proof);
            Ok(Json(linked_proof))
        }
        None => {
            // TODO: we should try to see if we have a snark maybe and start from there?
            // storage.get_snark(), then do + 1 and try that proof from fri_storage, instead of starting from 1
            let key = "1".to_string();
            tracing::info!("LinkedProof not found, trying to start a new LinkedProof from block: {}", key);
            state.get_fri_proof(&"1".to_string()).await.map(|proof| Json(LinkedProof { start_block: 1, end_block_inclusive: 1, proof })).ok_or(StatusCode::NOT_FOUND)
        }
    }
}

pub async fn put(State(state): State<AppState>, Json(linked_proof): Json<LinkedProof>) -> Result<StatusCode, StatusCode> {
    tracing::info!("linked/put({linked_proof:?})");
    state.put_linked_proof(linked_proof).await;
    Ok(StatusCode::CREATED)
}
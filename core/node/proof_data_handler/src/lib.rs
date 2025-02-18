#[cfg(test)]
mod tests;

mod errors;
mod metrics;
mod middleware;
mod proof_data_submitter;
mod rpc_client;
mod tee_proof_api;

pub use proof_data_submitter::{
    proof_data_processor::ProofGenerationDataProcessor, ProofGenerationDataSubmitter,
};
pub use rpc_client::{processor::ProofDataProcessor, RpcClient};
pub use tee_proof_api::{RequestProcessor, TeeProofDataHandler};

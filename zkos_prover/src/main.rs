use air_compiler_cli::{
    prover_utils::{
        create_proofs_internal, create_recursion_proofs, load_binary_from_path,
        program_proof_from_proof_list_and_metadata, GpuSharedState,
    },
    Machine,
};
use anyhow::{anyhow, Result};
use base64;
use execution_utils::ProgramProof;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Response from GET /next-block
#[derive(Debug, Deserialize)]
struct NextBlockResponse {
    block_number: u64,
    block_data: String,
}

/// Payload for POST /submit-proof
#[derive(Debug, Serialize)]
struct ProofPayload {
    block_number: u64,
    proof: String,
}

/// Response from POST /submit-proof
#[derive(Debug, Deserialize)]
struct ProofResult {
    result: String,
}

/// HTTP client for the proof-data server
#[derive(Clone)]
pub struct ProofDataClient {
    http: Client,
    base_url: String,
}

impl ProofDataClient {
    /// Create a new client targeting the given base URL (e.g., "http://localhost:3000")
    pub fn new<U: Into<String>>(base_url: U) -> Self {
        ProofDataClient {
            http: Client::new(),
            base_url: base_url.into(),
        }
    }

    /// Fetch the next block to prove.
    /// Returns `Ok(None)` if there's no block pending (204 No Content).
    pub async fn get_next_block(&self) -> Result<Option<(u64, Vec<u8>)>> {
        let url = format!("{}/next-block", self.base_url);
        let resp = self.http.get(&url).send().await?;

        match resp.status() {
            StatusCode::OK => {
                let body: NextBlockResponse = resp.json().await?;
                let data = base64::decode(&body.block_data)
                    .map_err(|e| anyhow!("Failed to decode block data: {}", e))?;
                Ok(Some((body.block_number, data)))
            }
            StatusCode::NO_CONTENT => Ok(None),
            s => Err(anyhow!("Unexpected status {} when fetching next block", s)),
        }
    }

    /// Submit a proof for the processed block
    /// Returns the vector of u32 as returned by the server.
    pub async fn submit_proof(&self, block_number: u64, proof: String) -> Result<String> {
        let url = format!("{}/submit-proof", self.base_url);
        let payload = ProofPayload {
            block_number,
            proof,
        };

        let resp = self.http.post(&url).json(&payload).send().await?;

        if resp.status().is_success() {
            let body: ProofResult = resp.json().await?;
            Ok(body.result)
        } else {
            Err(anyhow!(
                "Server returned {} when submitting proof",
                resp.status()
            ))
        }
    }
}

fn create_proof(
    prover_input: Vec<u32>,
    binary: &Vec<u32>,
    gpu_state: &mut GpuSharedState,
) -> ProgramProof {
    let (proof_list, proof_metadata) = create_proofs_internal(
        binary,
        prover_input,
        &Machine::Standard,
        // FIXME: figure out how many instances (currently gpu ignores this).
        100,
        None,
        #[cfg(feature = "gpu")]
        &mut Some(gpu_state),
        #[cfg(not(feature = "gpu"))]
        &mut None,
    );
    let basic_proofs = proof_list.basic_proofs.len();
    let delegation_proofs = proof_list
        .delegation_proofs
        .iter()
        .map(|x| x.1.len())
        .collect::<Vec<_>>();
    let (recursion_proof_list, recursion_proof_metadata, _) = create_recursion_proofs(
        proof_list,
        proof_metadata,
        &None,
        #[cfg(feature = "gpu")]
        &mut Some(gpu_state),
        #[cfg(not(feature = "gpu"))]
        &mut None,
    );

    program_proof_from_proof_list_and_metadata(&recursion_proof_list, &recursion_proof_metadata)
}

#[tokio::main]
pub async fn main() {
    let client = ProofDataClient::new("http://localhost:3124");

    let binary = load_binary_from_path(&"app.bin".to_string());
    let mut gpu_state = GpuSharedState::default();
    #[cfg(feature = "gpu")]
    gpu_state.preheat_for_universal_verifier(&binary);

    println!("Starting Zksync OS prover for {}", client.base_url);

    loop {
        let (block_number, prover_input) = match client.get_next_block().await.unwrap() {
            Some(next_block) => next_block,
            None => continue,
        };

        // make prover_input (Vec<u8>) into Vec<u32>:
        let prover_input: Vec<u32> = prover_input
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();

        println!(
            "{:?} starting proving block number {}",
            SystemTime::now(),
            block_number
        );

        let proof = create_proof(prover_input, &binary, &mut gpu_state);
        println!(
            "{:?} finished proving block number {}",
            SystemTime::now(),
            block_number
        );

        let serialized_proof = serde_json::to_string(&proof).unwrap();
        let _ = client
            .submit_proof(block_number, serialized_proof)
            .await
            .expect("Failed to submit a proof");
        println!(
            "{:?} submitted proof for block number {}",
            SystemTime::now(),
            block_number
        );
    }
}

use air_compiler_cli::{
    prover_utils::{
        create_proofs_internal, create_recursion_proofs, load_binary_from_path,
        program_proof_from_proof_list_and_metadata, GpuSharedState,
    },
    Machine,
};
use anyhow::{anyhow, Result};
use base64;
use clap::Parser;
use execution_utils::ProgramProof;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::thread::sleep;
use std::time::SystemTime;

/// Command-line arguments for the Zksync OS prover
#[derive(Parser, Debug)]
#[command(name = "Zksync OS Prover")]
#[command(version = "1.0")]
#[command(about = "Prover for Zksync OS", long_about = None)]
struct Args {
    /// Base URL for the proof-data server (e.g., "http://<IP>:<PORT>")
    #[arg(short, long, default_value = "http://localhost:3124")]
    base_url: String,
    /// Enable logging and use the logging-enabled binary
    #[arg(long)]
    enabled_logging: bool,
}

// Note: copied from zkos_prover_input_generator.rs
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
    pub async fn pick_next_prover_job(&self) -> Result<Option<(u32, Vec<u8>)>> {
        let url = format!("{}/prover-jobs/FRI/pick", self.base_url);
        let resp = self.http.post(&url).send().await?;

        match resp.status() {
            StatusCode::OK => {
                let body: NextProverJobPayload = resp.json().await?;
                let data = base64::decode(&body.prover_input)
                    .map_err(|e| anyhow!("Failed to decode block data: {}", e))?;
                Ok(Some((body.block_number, data)))
            }
            StatusCode::NO_CONTENT => Ok(None),
            s => Err(anyhow!("Unexpected status {} when fetching next block", s)),
        }
    }

    /// Submit a proof for the processed block
    /// Returns the vector of u32 as returned by the server.
    pub async fn submit_proof(&self, block_number: u32, proof: String) -> Result<()> {
        let url = format!("{}/prover-jobs/FRI/submit", self.base_url);
        let payload = ProofPayload {
            block_number,
            proof,
        };

        let resp = self.http.post(&url).json(&payload).send().await?;

        if resp.status().is_success() {
            Ok(())
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
    let (recursion_proof_list, recursion_proof_metadata) = create_recursion_proofs(
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
    let args = Args::parse();

    let client = ProofDataClient::new(args.base_url);

    let binary_path = if args.enabled_logging {
        "../execution_environment/app_logging_enabled.bin".to_string()
    } else {
        "../execution_environment/app.bin".to_string()
    };

    let binary = load_binary_from_path(&binary_path.to_string());
    let mut gpu_state = GpuSharedState::default();
    #[cfg(feature = "gpu")]
    {
        gpu_state.preheat_for_universal_verifier(&binary);
        gpu_state.enable_multigpu();
    }

    println!("Starting Zksync OS FRI prover for {}", client.base_url);

    loop {
        let (block_number, prover_input) = match client.pick_next_prover_job().await {
            Err(err) => {
                eprintln!("Error fetching next prover job: {}", err);
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                continue;
            }
            Ok(Some(next_block)) => next_block,
            Ok(None) => {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                continue;
            }
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
        let proof_bytes: Vec<u8> =
            bincode::serialize(&proof).expect("failed to bincode-serialize proof");

        // 2) base64-encode that binary blob
        let proof_b64 = base64::encode(&proof_bytes);

        match client.submit_proof(block_number, proof_b64).await {
            Ok(_) => println!(
                "{:?} successfully submitted proof for block number {}",
                SystemTime::now(),
                block_number
            ),
            Err(err) => {
                eprintln!(
                    "{:?} failed to submit proof for block number {}: {}",
                    SystemTime::now(),
                    block_number,
                    err
                );
            }
        }
    }
}

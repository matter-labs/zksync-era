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
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Parser, Debug)]
#[command(name = "ZKsync OS Prover")]
#[command(version = "0.0.1")]
#[command(about = "FRI prover for ZKsync OS", long_about = None)]
struct Cli {
    /// Sequencer URL to submit proofs to (i.e., "http://<IP>:<PORT>")
    #[arg(short, long, default_value = "http://localhost:3124", value_name = "URL")]
    url: String,

    /// Activate verbose logging (`-v`, `-vv`, ...)
    #[arg(short, long, required = true, action = clap::ArgAction::Count)]
    verbose: u8,

    ///
    #[arg(short, long, default_value = "../app.bin", value_name = "APP_BINARY_PATH")]
    binary_path: PathBuf,
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

fn init_tracing(verbosity: u8) {
    use tracing_subscriber::{fmt, EnvFilter};
    let level = match verbosity {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));
    fmt::Subscriber::builder()
        .with_env_filter(env_filter)
        .init();
}

#[tokio::main]
pub async fn main() {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    let client = ProofDataClient::new(&cli.url);

    let bin_path = String::from(cli.binary_path.to_str()
        .expect("Failed to convert binary path to string"));

    let binary = load_binary_from_path(&bin_path);
    tracing::debug!("Loaded binary from path: {}", bin_path);
    let mut gpu_state = GpuSharedState::default();
    #[cfg(feature = "gpu")]
    {
        gpu_state.preheat_for_universal_verifier(&binary);
        gpu_state.enable_multigpu();
    }
    tracing::info!("Starting ZKsync OS FRI prover connected to {}", cli.url);

    loop {
        tracing::info!("Getting FRI job...");
        let (block_number, prover_input) = match client.pick_next_prover_job().await {
            Err(err) => {
                let duration = tokio::time::Duration::from_millis(1000);
                tracing::error!("Failed getting FRI job: {:?}, bouncing back for {duration:?}", err);
                tokio::time::sleep(duration).await;
                continue;
            }
            Ok(Some(next_block)) => next_block,
            Ok(None) => {
                let duration = tokio::time::Duration::from_millis(100);
                tracing::debug!("No FRI job available, sleeping for {duration:?}");
                tokio::time::sleep(duration).await;
                continue;
            }
        };
        tracing::info!("Received FRI job for block number {}", block_number);

        // make prover_input (Vec<u8>) into Vec<u32>:
        let prover_input: Vec<u32> = prover_input
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();

        tracing::info!(
            "Starting proving block number {}",
            block_number
        );

        let proof = create_proof(prover_input, &binary, &mut gpu_state);
        tracing::info!("Finished proving block number {}",
            block_number
        );

        tracing::info!("Sending proof for block number {} to sequencer at {}", block_number, cli.url);

        let proof_bytes: Vec<u8> =
            bincode::serialize(&proof).expect("failed to bincode-serialize proof");

        // 2) base64-encode that binary blob
        let proof_b64 = base64::encode(&proof_bytes);

        match client.submit_proof(block_number, proof_b64).await {
            Ok(_) => tracing::info!(
                "Submitted proof for block number {} to sequencer at {}",
                block_number,
                cli.url
            ),
            Err(err) => {
                tracing::error!(
                    "Failed to submit proof for block number {}: {}",
                    block_number,
                    err
                );
            }
        }
    }
}


///// NEW PROVERS BELOW, CURRENTLY WIP

// use air_compiler_cli::prover_utils::{load_binary_from_path, GpuSharedState};
// use clap::Parser;
// use zksync_os_prover::fri_executor::FriExecutor;
//
// #[derive(Parser)]
// #[command(author, version, about, long_about = None)]
// struct Cli {
//     /// Sequencer URL to submit proofs to
//     #[arg(short, long, required = true, value_name = "URL")]
//     url: String,
//
//     /// Activate verbose logging (`-v`, `-vv`, ...)
//     #[arg(short, long, global = true, action = clap::ArgAction::Count)]
//     verbose: u8,
//
//     // #[command(subcommand)]
//     // command: Commands,
// }
//
// // #[derive(Subcommand)]
// // enum Commands {
// //     /// Picks the next FRI proof job from the sequencer; sequencer marks job as picked (and will not give it to other clients, until the job expires)
// //     PickFri {},
// //     /// Submits block's FRI proof to sequencer
// //     SubmitFri {},
// //     /// Picks the next SNARK proof job from the sequencer; sequencer marks job as picked (and will not give it to other clients, until the job expires)
// //     PickSnark {},
// //     /// Submits block's SNARK proof to sequencer
// //     SubmitSnark {
// //         /// Block number to submit the SNARK proof for
// //         #[arg(short, long, value_name = "BLOCK")]
// //         block: u32,
// //         /// Path to the SNARK proof file to submit
// //         #[arg(short, long, value_name = "SNARK_PATH")]
// //         path: String,
// //     },
// // }
//
// fn init_tracing(verbosity: u8) {
//     use tracing_subscriber::{fmt, EnvFilter};
//     let level = match verbosity {
//         0 => "info",
//         1 => "debug",
//         _ => "trace",
//     };
//     let env_filter = EnvFilter::try_from_default_env()
//         .unwrap_or_else(|_| EnvFilter::new(level));
//     fmt::Subscriber::builder()
//         .with_env_filter(env_filter)
//         .init();
// }
//
// fn main() -> anyhow::Result<()> {
//     let cli = Cli::parse();
//     init_tracing(cli.verbose);
//
//     // TODO: Maybe allow this to be configured optionally via CLI?
//     // TODO: dumb API, change this to accept &str as well
//     let binary = load_binary_from_path(&String::from("../app.bin"));
//
//     let mut gpu_state = GpuSharedState::default();
//     gpu_state.preheat_for_universal_verifier(&binary);
//     gpu_state.enable_multigpu();
//
//     let picker = SequencerJobPicker::new(&cli.url);
//     let saver = SequencerJobSaver::new(&cli.url);
//     let executor = FriExecutor::new(gpu_state, binary);
//
//
//     Ok(())
// }

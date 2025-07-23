use clap::{Parser, Subcommand, ValueEnum};
// use proof_cache::client::ProofCacheClient;
use sequencer_proof_client::{SequencerProofClient, SnarkProofInputs};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::time::{Duration, Instant};
use tracing_subscriber::{EnvFilter, FmtSubscriber};
use zkos_wrapper::{prove, serialize_to_file, SnarkWrapperProof};
use zksync_airbender_cli::prover_utils::{create_final_proofs_from_program_proof, create_proofs_internal, generate_oracle_data_from_metadata_and_proof_list, load_binary_from_path, program_proof_from_proof_list_and_metadata, proof_list_and_metadata_from_program_proof, GpuSharedState, VerifierCircuitsIdentifiers};
use zksync_airbender_cli::Machine;
use zksync_airbender_execution_utils::ProgramProof;
// use zksync_os_snark_prover::client::SequencerProofClient;
// use zksync_os_snark_prover::single_fri_snark_executor::{deserialize_from_file, SingleFriSnarkExecutor};
// use zksync_os_snark_prover::snark_job_pickers::SequencerSingleFriSnarkJobPicker;
// use zksync_os_snark_prover::snark_job_savers::SnarkJobSaver;
use zksync_prover_job_processor::{Executor, JobPicker, JobSaver};

// #[derive(Debug, Clone, Default, ValueEnum)]
// enum SnarkMode {
//     SingleFri,
//     #[default]
//     LinkingFris,
// }

#[derive(Default, Debug, Serialize, Deserialize, Parser, Clone)]
pub struct SetupOptions {
    #[arg(long)]
    binary_path: String,

    #[arg(long)]
    output_dir: String,

    #[arg(long)]
    trusted_setup_file: Option<String>,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    // TODO: redo this command, naming is confusing
    /// Generate the snark verification keys
    GenerateKeys {
        #[clap(flatten)]
        setup: SetupOptions,
        /// Path to the output verification key file
        #[arg(long)]
        vk_verification_key_file: Option<String>,
    },

    RunProver {
        #[arg(short, long)]
        sequencer_url: Option<String>,
        #[clap(flatten)]
        setup: SetupOptions,
        // #[arg(short, long, default_value = "linking-fris")]
        // mode: SnarkMode,
    },
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    FmtSubscriber::builder()
        .with_env_filter(filter)
        .init();
}

fn generate_verification_key(binary_path: String, output_dir: String, trusted_setup_file: Option<String>, vk_verification_key_file: Option<String>) {
    match zkos_wrapper::generate_vk(binary_path, output_dir, trusted_setup_file, true) {
        Ok(key) => {
            if let Some(vk_file) = vk_verification_key_file {
                std::fs::write(vk_file, format!("{:?}", key))
                    .expect("Failed to write verification key to file");
            } else {
                tracing::info!("Verification key generated successfully: {:#?}", key);
            }
        }
        Err(e) => {
            tracing::info!("Error generating keys: {e}");
        }
    }
}

fn main() {
    init_tracing();
    let cli = Cli::parse();

    match cli.command {
        Commands::GenerateKeys {
            setup:
            SetupOptions {
                binary_path,
                output_dir,
                trusted_setup_file,
            },
            vk_verification_key_file,
        } => generate_verification_key(binary_path, output_dir, trusted_setup_file, vk_verification_key_file),
        Commands::RunProver {
            sequencer_url,
            setup:
            SetupOptions {
                binary_path,
                output_dir,
                trusted_setup_file,
            },
            // mode,
        } => {
            // TODO: edit this comment
            // we need a bigger stack, due to crypto code exhausting default stack size, 40 MBs picked here
            // note that size is not allocated, only limits the amount to which it can grow
            let stack_size = 40 * 1024 * 1024;
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .thread_stack_size(stack_size)
                .enable_all()
                .build()
                .expect("failed to build tokio context");
            runtime.block_on(run_snarker(
                sequencer_url,
                binary_path,
                output_dir,
                trusted_setup_file,
                // mode,
            ));
        }
    }
}

// async fn run_single_fri_snark(sequencer_url: Option<String>,
//                               binary_path: String,
//                               output_dir: String,
//                               trusted_setup_file: Option<String>, ) {
//     let sequencer_url = sequencer_url.unwrap_or("http://localhost:3124".to_string());
//
//     tracing::info!(
//         "Starting zksync_os_snark_prover with sequencer at {:?}",
//         &sequencer_url
//     );
//     let mut picker = SequencerSingleFriSnarkJobPicker::new(sequencer_url.clone());
//     let saver = SnarkJobSaver::new(sequencer_url);
//
//     loop {
//         tracing::info!("Started picking a job");
//         let job = match picker.pick_job().await {
//             Err(e) => {
//                 tracing::warn!("Failed picking job: {e:?}");
//                 tokio::time::sleep(Duration::from_millis(250)).await;
//                 continue;
//             }
//             Ok(data) => data,
//         };
//         let (proof, block) = match job {
//             Some(data) => data,
//             None => {
//                 tracing::info!("No jobs found");
//                 tokio::time::sleep(Duration::from_millis(250)).await;
//                 continue;
//             }
//         };
//         tracing::info!("Finished picking job for block {block:?}");
//
//         let metadata = block.clone();
//
//         tracing::info!("Started executing job for block {block:?}");
//         let executor = SingleFriSnarkExecutor {
//             binary_path: binary_path.clone(),
//             output_dir: output_dir.clone(),
//             trusted_setup_file: trusted_setup_file.clone(),
//         };
//
//         let result = tokio::task::spawn_blocking(move || executor.execute(proof, metadata))
//             .await
//             .expect("failed executing");
//
//         let snark_proof = match result {
//             Err(e) => {
//                 tracing::warn!("Failed executing job for block {block:?}: {e:?}");
//                 continue;
//             }
//             Ok(data) => {
//                 tracing::info!("Finished executing job for block {block:?}");
//                 data
//             }
//         };
//
//         tracing::info!("Started submitting proof to sequencer for block {block:?}");
//         let res = saver
//             .save_job_result((Ok(snark_proof), block.clone()))
//             .await;
//         match res {
//             Ok(()) => {
//                 tracing::info!("Finished submitting proof to sequencer for block {block:?}");
//             }
//             Err(e) => {
//                 tracing::warn!("Failed submitting proof to sequencer for block {block:?}: {e:?}");
//             }
//         }
//     }
// }

fn merge_fris(snark_proof_input: SnarkProofInputs, verifier_binary: &Vec<u32>, gpu_state: &mut GpuSharedState) -> ProgramProof {
    if snark_proof_input.fri_proofs.len() == 1 {
        tracing::info!("No proof merging needed, only one proof provided");
        return snark_proof_input.fri_proofs[0].clone();
    }
    tracing::info!("Starting proof merging");

    let mut proof = snark_proof_input.fri_proofs[0].clone();
    for i in 1..snark_proof_input.fri_proofs.len() {
        let up_to_block = snark_proof_input.from_block_number.0 + i as u32 - 1;
        let curr_block = snark_proof_input.from_block_number.0 + i as u32;
        tracing::info!("Linking proofs up to {} with proof for block {}", up_to_block, curr_block);
        let second_proof = snark_proof_input.fri_proofs[i].clone();

        let (first_metadata, first_proof_list) = proof_list_and_metadata_from_program_proof(proof);
        let (second_metadata, second_proof_list) = proof_list_and_metadata_from_program_proof(second_proof);
        let first_oracle = generate_oracle_data_from_metadata_and_proof_list(&first_metadata, &first_proof_list);
        let second_oracle = generate_oracle_data_from_metadata_and_proof_list(&second_metadata, &second_proof_list);

        let mut merged_input = vec![VerifierCircuitsIdentifiers::CombinedRecursionLayers as u32];
        merged_input.extend(first_oracle);
        merged_input.extend(second_oracle);

        let (current_proof_list, proof_metadata) = create_proofs_internal(
            &verifier_binary,
            merged_input,
            &Machine::Reduced,
            100, // Guessing - FIXME!!
            Some(first_metadata.create_prev_metadata()),
            &mut Some(gpu_state),
            &mut Some(0f64),
        );
        proof = program_proof_from_proof_list_and_metadata(&current_proof_list, &proof_metadata);
        tracing::info!("Finished linking proofs up to block {}", up_to_block);
    }
    // TODO: We can do a recursion step here as well, IIUC
    tracing::info!("Finishing linking all proofs from {} to {}", snark_proof_input.from_block_number, snark_proof_input.to_block_number);
    proof
}

async fn run_linking_fri_snark(
    sequencer_url: Option<String>,
    binary_path: String,
    output_dir: String,
    trusted_setup_file: Option<String>,
) -> anyhow::Result<()> {
    let sequencer_url = sequencer_url.unwrap_or("http://localhost:3124".to_string());
    let sequencer_client = SequencerProofClient::new(sequencer_url.clone());

    tracing::info!(
        "Starting zksync_os_snark_prover"
    );
    let verifier_binary = load_binary_from_path(&"/home/evl/code/zksync-airbender/tools/verifier/universal.bin".to_string());
    let mut gpu_state = GpuSharedState::new(&verifier_binary);

    loop {
        let proof_time = Instant::now();
        tracing::info!("Started picking job");
        let snark_proof_input = match sequencer_client.pick_snark_job().await {
            Ok(Some(snark_proof_input)) => {
                if snark_proof_input.fri_proofs.len() == 0 {
                    let err_msg = "No FRI proofs were sent, issue with Prover API/Sequencer, quitting...";
                    tracing::error!(err_msg);
                    return Err(anyhow::anyhow!(err_msg));
                }
                snark_proof_input
            }
            Ok(None) => {
                tracing::info!("No SNARK jobs found, retrying in 5s");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
            Err(_) => {
                tracing::error!("Failed to pick SNARK job, retrying in 30s");
                tokio::time::sleep(Duration::from_secs(30)).await;
                continue;
            }
        };
        let start_block = snark_proof_input.from_block_number;
        let end_block = snark_proof_input.to_block_number;
        tracing::info!("Finished picking job, will aggregate from {} to {} inclusive", start_block, end_block);

        let proof = merge_fris(snark_proof_input, &verifier_binary, &mut gpu_state);

        tracing::info!("Creating final proof before SNARKification");
        let final_proof = create_final_proofs_from_program_proof(proof);

        tracing::info!("Finished creating final proof");
        let one_fri_path = Path::new(&output_dir).join("one_fri.tmp");

        serialize_to_file(&final_proof, &one_fri_path);

        tracing::info!("SNARKifying proof");
        let snark_time = Instant::now();
        match prove(
            one_fri_path.into_os_string().into_string().unwrap(),
            Some(binary_path.clone()),
            output_dir.clone(),
            trusted_setup_file.clone(),
            false,
        ) {
            Ok(()) => {
                tracing::info!(
                    "SNARKification took {:?}, with total proving time being {:?}",
                    snark_time.elapsed(),
                    proof_time.elapsed()
                );
            }
            Err(e) => {
                tracing::info!("failed to SNARKify proof: {e:?}");
            }
        }
        let snark_proof: SnarkWrapperProof = deserialize_from_file(
            Path::new(&output_dir)
                .join("snark_proof.json")
                .to_str()
                .unwrap(),
        );
        sequencer_client.submit_snark_proof(start_block, end_block, snark_proof).await?
    }
}

async fn run_snarker(
    sequencer_url: Option<String>,
    binary_path: String,
    output_dir: String,
    trusted_setup_file: Option<String>,
    // mode: SnarkMode,
) {
    // tracing::info!("SNARK mode {:?}", mode);
    // match mode {
    //     SnarkMode::SingleFri => {
    //         run_single_fri_snark(sequencer_url, binary_path, output_dir, trusted_setup_file).await;
    //     }
    //     SnarkMode::LinkingFris => {
    run_linking_fri_snark(sequencer_url, binary_path, output_dir, trusted_setup_file).await.expect("failed whilst running SNARK prover");
    //     }
    // }
}

fn deserialize_from_file<T: serde::de::DeserializeOwned>(filename: &str) -> T {
    let src = std::fs::File::open(filename).unwrap();
    serde_json::from_reader(src).unwrap()
}
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use zksync_os_snark_prover::snark_executor::SnarkExecutor;
use zksync_os_snark_prover::snark_job_pickers::SnarkJobPicker;
use zksync_os_snark_prover::snark_job_savers::SnarkJobSaver;
use zksync_prover_job_processor::{Executor, JobPicker, JobSaver};

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
    /// Generate the snark verification keys
    GenerateKeys {
        #[clap(flatten)]
        setup: SetupOptions,
    },

    RunProver {
        #[arg(short, long)]
        sequencer_url: Option<String>,
        #[clap(flatten)]
        setup: SetupOptions,
    },
}

fn main() {
    // we need a bigger stack, due to crypto code exhausting default stack size, 30 gigs picked here
    // note that size is not allocated, but only limits the amount to which it can grow
    // 30 gigs has been picked as a super safe error-margin, much lower should be fine (I assume some 20MB)
    let stack_size = 30 * 1024 * 1024 * 1024;
    let cli = Cli::parse();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .thread_stack_size(stack_size)
        .enable_all()
        .build()
        .expect("failed to build tokio context");

    match cli.command {
        Commands::GenerateKeys {
            setup:
                SetupOptions {
                    binary_path,
                    output_dir,
                    trusted_setup_file,
                },
        } => {
            if let Err(e) =
                zkos_wrapper::generate_vk(binary_path, output_dir, trusted_setup_file, true, false)
            {
                println!("Error generating keys: {e}");
            }
        }
        Commands::RunProver {
            sequencer_url,
            setup:
                SetupOptions {
                    binary_path,
                    output_dir,
                    trusted_setup_file,
                },
        } => {
            runtime.block_on(run_prover(
                sequencer_url,
                binary_path,
                output_dir,
                trusted_setup_file,
            ));
        }
    }
}

async fn run_prover(
    sequencer_url: Option<String>,
    binary_path: String,
    output_dir: String,
    trusted_setup_file: Option<String>,
) {
    let sequencer_url = sequencer_url.unwrap_or("http://localhost:3124".to_string());

    println!(
        "Starting jack in the box with sequencer at {:?}",
        &sequencer_url
    );
    let mut picker = SnarkJobPicker::new(sequencer_url.clone());
    let saver = SnarkJobSaver::new(sequencer_url);

    loop {
        println!("Started picking a job");
        let job = match picker.pick_job().await {
            Err(e) => {
                println!("Failed picking job: {e:?}");
                tokio::time::sleep(Duration::from_millis(250)).await;
                // return;
                continue;
            }
            Ok(data) => data,
        };
        let (proof, block) = match job {
            Some(data) => data,
            None => {
                println!("No jobs found");
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
        };
        println!("Finished picking job for block {block:?}");

        let b = block.clone();

        let metadata = b;

        println!("Started executing job for block {block:?}");
        let executor = SnarkExecutor {
            binary_path: binary_path.clone(),
            output_dir: output_dir.clone(),
            trusted_setup_file: trusted_setup_file.clone(),
        };

        let result = tokio::task::spawn_blocking(move || executor.execute(proof, metadata))
            .await
            .expect("failed executing");

        let snark_proof = match result {
            Err(e) => {
                println!("Failed executing job for block {block:?}: {e:?}");
                // return;
                continue;
            }
            Ok(data) => {
                println!("Finished executing job for block {block:?}");
                data
            }
        };

        println!("Started submitting proof to sequencer for block {block:?}");
        let res = saver
            .save_job_result((Ok(snark_proof), block.clone()))
            .await;
        match res {
            Ok(()) => {
                println!("Finished submitting proof to sequencer for block {block:?}");
            }
            Err(e) => {
                println!("Failed submitting proof to sequencer for block {block:?}: {e:?}");
            }
        }
    }
}

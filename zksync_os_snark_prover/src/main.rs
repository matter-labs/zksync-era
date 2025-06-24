use clap::Parser;
use std::time::Duration;
use zksync_os_snark_prover::snark_executor::SnarkExecutor;
use zksync_os_snark_prover::snark_job_pickers::SnarkJobPicker;
use zksync_os_snark_prover::snark_job_savers::SnarkJobSaver;
use zksync_prover_job_processor::{Executor, JobPicker, JobSaver};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    sequencer_url: Option<String>,

    #[arg(long)]
    binary_path: String,

    #[arg(long)]
    output_dir: String,

    #[arg(long)]
    trusted_setup_file: Option<String>,
}

fn main() {
    // we need a bigger stack, due to crypto code exhausting default stack size, 30 gigs picked here
    // note that size is not allocated, but only limits the amount to which it can grow
    // 30 gigs has been picked as a super safe error-margin, much lower should be fine (I assume some 20MB)
    let stack_size = 30 * 1024 * 1024 * 1024;

    tokio::runtime::Builder::new_multi_thread()
        .thread_stack_size(stack_size)
        .enable_all()
        .build()
        .expect("failed to build tokio context")
        .block_on(main_entrypoint())
}

async fn main_entrypoint() {
    let cli = Cli::parse();

    let sequencer_url = cli
        .sequencer_url
        .unwrap_or("http://localhost:3124".to_string());

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
            binary_path: cli.binary_path.clone(),
            output_dir: cli.output_dir.clone(),
            trusted_setup_file: cli.trusted_setup_file.clone(),
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

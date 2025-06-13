// mod snark_executor;
// mod snark_job_pickers;
// mod snark_job_savers;

use std::time::Duration;
use zksync_os_snark_prover::snark_executor::SnarkExecutor;
use zksync_os_snark_prover::snark_job_pickers::SnarkJobPicker;
use zksync_os_snark_prover::snark_job_savers::SnarkJobSaver;
use zksync_prover_job_processor::{Executor, JobPicker, JobSaver};

fn deserialize_from_file<T: serde::de::DeserializeOwned>(filename: &str) -> T {
    let src = std::fs::File::open(filename).unwrap();
    serde_json::from_reader(src).unwrap()
}

// fn init_tracing(verbose: bool) {
//     use tracing_subscriber::{fmt, EnvFilter};
//     let level = if verbose "trace" else "info";
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
    // tracing_subscriber::fmt::init();

    // Builder::new().filter_level(LevelFilter::Info).init();
    //
    // log::info!("Started jack in the box");
    // ObservabilityConfig::new
    // #[derive(Debug, Clone, PartialEq, Deserialize)]
    // pub struct ObservabilityConfig {
    //     /// URL of the Sentry instance to send events to.
    //     pub sentry_url: Option<String>,
    //     /// Name of the environment to use in Sentry.
    //     pub sentry_environment: Option<String>,
    //     /// Opentelemetry configuration.
    //     pub opentelemetry: Option<OpentelemetryConfig>,
    //     /// Format of the logs as expected by the `vlog` crate.
    //     /// Currently must be either `plain` or `json`.
    //     pub log_format: String,
    //     /// Log directives in format that is used in `RUST_LOG`
    //     pub log_directives: Option<String>,
    // }

    let sequencer_url = String::from("http://64.227.120.222:3124");

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
                // return;
            }
        };
        println!("Finished picking job for block {block:?}");

        let b = block.clone();

        println!("Started executing job for block {block:?}");
        let executor = SnarkExecutor;

        let result = tokio::task::spawn_blocking(move || executor.execute(proof, b))
            .await
            .expect("failed executing");
        // let result = tokio::task::spawn_blocking(async {
        //     executor.execute(proof, block)
        // }).await;
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
        let res = saver.save_job_result((Ok(snark_proof), block)).await;
        match res {
            Ok(()) => {
                println!("Finished submitting proo fto sequencer for block {block:?}");
            }
            Err(e) => {
                println!("Failed submitting proof to sequencer for block {block:?}: {e:?}");
            }
        }
    }

    // // loop {
    // println!("Started picking a job");
    // let job = match picker.pick_job().await {
    //     Err(e) => {
    //         println!("Failed picking job: {e:?}");
    //         tokio::time::sleep(Duration::from_millis(250)).await;
    //         return;
    //         // continue;
    //     }
    //     Ok(data) => data,
    // };
    // let (proof, block) = match job {
    //     Some(data) => data,
    //     None => {
    //         println!("No jobs found");
    //         tokio::time::sleep(Duration::from_millis(250)).await;
    //         // continue;
    //         return;
    //     }
    // };
    // println!("Finished picking job for block {block:?}");
    //
    // let b = block.clone();
    //
    // println!("Started executing job for block {block:?}");
    // let executor = SnarkExecutor;
    //
    // let result = tokio::task::spawn_blocking(move || executor.execute(proof, b))
    //     .await
    //     .expect("failed executing");
    // // let result = tokio::task::spawn_blocking(async {
    // //     executor.execute(proof, block)
    // // }).await;
    // let snark_proof = match result {
    //     Err(e) => {
    //         println!("Failed executing job for block {block:?}: {e:?}");
    //         return;
    //         // continue;
    //     }
    //     Ok(data) => {
    //         println!("Finished executing job for block {block:?}");
    //         data
    //     }
    // };
    //
    // println!("Started submitting proof to sequencer for block {block:?}");
    // let res = saver.save_job_result((Ok(snark_proof), block)).await;
    // match res {
    //     Ok(()) => {
    //         println!("Finished submitting proo fto sequencer for block {block:?}");
    //     }
    //     Err(e) => {
    //         println!("Failed submitting proof to sequencer for block {block:?}: {e:?}");
    //     }
    // }

    // }

    // let runner = JobRunner::new(executor, picker, saver, 1, None);
    // let tasks = runner.run();
    // let mut tasks = ManagedTasks::new(tasks);
    // tracing::info!("a");
    // tasks.wait_single().await;
    // tracing::info!("b");
}

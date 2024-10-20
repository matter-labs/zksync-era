// use std::path::PathBuf;
//
// use clap::Parser;
//
// #[derive(Debug, Parser)]
// #[command(author = "Matter Labs", version)]
// #[command(name = "circuit_prover")]
// #[command(about = "CLI for running circuit provers")]
// struct Cli {
//     /// Path to file configuration
//     #[arg(short = 'c', long)]
//     config_path: Option<PathBuf>,
//     /// Path to file secrets
//     #[arg(short = 's', long)]
//     secrets_path: Option<PathBuf>,
//     /// Number of light witness vector generators to run.
//     /// Corresponds to 1 CPU thread & ~2GB of RAM.
//     #[arg(short = 'l', long, default_value = 1)]
//     light_wvg_count: usize,
//     /// Number of heavy witness vector generators to run.
//     /// Corresponds to 1 CPU thread & ~9GB of RAM.
//     #[arg(short = 'h', long, default_value = 1)]
//     heavy_wvg_count: usize,
//     // TODO: add Max VRAM allocation
// }
//
// #[tokio::main]
// async fn main() -> anyhow::Result<()> {
//     let cli = Cli::parse();
//     // let light_wvg_job_runner = JobRunner::new();
//     // let heavy_wvg_job_runner = JobRunner::new();
//     // let circuit_prover_job_runner = JobRunner::new();
//     // let executor = EmailExecutor;
//     // let picker = EmailJobPicker;
//     // let saver = EmailJobSaver;
//     // let num_workers = 4;
//     //
//     // let job_runner = JobRunner::new(executor, picker, saver, num_workers);
//     //
//     // job_runner.run().await?;
//
//     Ok(())
// }

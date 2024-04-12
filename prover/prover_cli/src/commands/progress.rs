use anyhow::Context as _;
use clap::Args as ClapArgs;
use prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_basic_types::L1BatchNumber;
use zksync_config::PostgresConfig;
use zksync_env_config::FromEnv;

#[derive(ClapArgs)]
pub(crate) struct Args {
    #[clap(short, long, conflicts_with = "all", required_unless_present = "all", num_args = 0..)]
    proof: Option<Vec<L1BatchNumber>>,
    #[clap(short, long, default_value("false"))]
    verbose: bool,
    #[clap(short, long, conflicts_with = "proof")]
    all: bool,
}

fn pretty_print_job_status(
    l1_batch_number: &L1BatchNumber,
    total_jobs: usize,
    successful_jobs: usize,
    failed_jobs: usize,
) {
    let progress = (successful_jobs as f32 / total_jobs as f32) * 100.0;
    println!("Batch number: {}", l1_batch_number);
    println!(
        "Progress: {:.2}% ({}/{})",
        progress, successful_jobs, total_jobs
    );
    println!("Failed: {}", failed_jobs);
}

async fn get_one_batch_progress(l1_batches_numbers: Vec<L1BatchNumber>) -> anyhow::Result<()> {
    let postgres_config = PostgresConfig::from_env().context("PostgresConfig::from_env()")?;

    let prover_connection_pool = ConnectionPool::<Prover>::builder(
        postgres_config.prover_url()?,
        postgres_config.max_connections()?,
    )
    .build()
    .await
    .context("failed to build a prover_connection_pool")?;

    let mut conn = prover_connection_pool.connection().await.unwrap();
    let stats = conn
        .fri_prover_jobs_dal()
        .get_prover_jobs_stats_for_batch(l1_batches_numbers)
        .await;

    for row in &stats {
        let (l1_batch_number, statistics) = row;
        let total_jobs =
            statistics.queued + statistics.in_progress + statistics.failed + statistics.successful;
        pretty_print_job_status(
            l1_batch_number,
            total_jobs,
            statistics.successful,
            statistics.failed,
        )
    }
    Ok(())
}

async fn get_all_batches_progress() -> anyhow::Result<()> {
    Ok(())
}
pub(crate) async fn run(args: Args) -> anyhow::Result<()> {
    println!("{:?}", args.proof);
    if let Some(l1_batch_number) = args.proof {
        get_one_batch_progress(l1_batch_number).await
    } else {
        get_all_batches_progress().await
    }
}

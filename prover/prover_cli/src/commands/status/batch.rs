use anyhow::{ensure, Context as _};
use clap::Args as ClapArgs;
use prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_basic_types::{prover_dal::JobCountStatistics, L1BatchNumber};
use zksync_config::PostgresConfig;
use zksync_env_config::FromEnv;

#[derive(ClapArgs)]
pub struct Args {
    #[clap(short, long, num_args = 0..)]
    batch: Vec<L1BatchNumber>,
    #[clap(short, long, default_value("false"))]
    verbose: bool,
}

pub(crate) async fn run(args: Args) -> anyhow::Result<()> {
    ensure!(
        !args.batch.is_empty(),
        "At least one batch number should be provided"
    );
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
        .get_prover_jobs_stats_for_batch(args.batch)
        .await;
    if stats.len() > 0 {
        for row in &stats {
            let (l1_batch_number, statistics) = row;

            pretty_print_job_status(l1_batch_number, statistics, args.verbose)
        }
    } else {
        println!("No batches found.")
    }
    Ok(())
}

fn pretty_print_job_status(
    l1_batch_number: &L1BatchNumber,
    statistics: &JobCountStatistics,
    verbose: bool,
) {
    let total_jobs =
        statistics.queued + statistics.in_progress + statistics.failed + statistics.successful;
    let progress = (statistics.successful as f32 / total_jobs as f32) * 100.0;
    println!("Batch number: {}", l1_batch_number);
    println!(
        "Progress: {:.2}% ({}/{})",
        progress, statistics.successful, total_jobs
    );
    if verbose {
        println!("In progress: {}", statistics.in_progress);
        println!("Queued: {}", statistics.in_progress);
        println!("Successful: {}", statistics.in_progress);
    }
    println!("Failed: {}", statistics.failed);
}

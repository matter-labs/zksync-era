use std::time::Duration;

use anyhow::Context;
use clap::Args as ClapArgs;
use prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_config::{configs::FriWitnessGeneratorConfig, PostgresConfig};
use zksync_env_config::FromEnv;

#[derive(ClapArgs)]
pub struct Args {
    /// Maximum number of attempts to requeue a job
    /// Default value is 0, which means that the job will be requeued no matter how many times it was attempted
    #[clap(long, default_value_t = 0)]
    max_attempts: u32,
    /// Requeue stuck jobs
    #[clap(long, default_value = "false", conflicts_with = "all")]
    stuck: bool,
    /// Requeue successful jobs
    #[clap(long, default_value = "false", conflicts_with = "all")]
    successful: bool,
    /// Requeue failed jobs
    #[clap(long, default_value = "false", conflicts_with = "all")]
    failed: bool,
    /// Requeue in progress jobs
    #[clap(long, default_value = "false", conflicts_with = "all")]
    in_progress: bool,
    /// Requeue all jobs
    #[clap(long, default_value = "false", conflicts_with_all(&["stuck", "successful", "failed", "in_progress"]))]
    all: bool,
}

pub async fn run(args: Args) -> anyhow::Result<()> {
    let postgres_config = PostgresConfig::from_env().context("PostgresConfig::from_env()")?;
    let fri_witness_generator_config =
        FriWitnessGeneratorConfig::from_env().context("FriWitnessGeneratorConfig::from_env()")?;

    let pool = ConnectionPool::<Prover>::builder(
        postgres_config.prover_url()?,
        postgres_config.max_connections()?,
    )
    .build()
    .await
    .context("failed to build a prover_connection_pool")?;

    if args.stuck || args.all {
        requeue_stuck_jobs(
            pool.clone(),
            fri_witness_generator_config
                .witness_generation_timeouts()
                .basic(),
            args.max_attempts,
        )
        .await?;
    }

    if args.successful || args.all {
        requeue_successful_jobs(pool.clone()).await?;
    }

    if args.failed || args.all {
        requeue_failed_jobs(pool.clone(), args.max_attempts).await?;
    }

    if args.in_progress || args.all {
        requeue_in_progress_jobs(
            pool,
            fri_witness_generator_config
                .witness_generation_timeouts()
                .basic(),
            args.max_attempts,
        )
        .await?;
    }

    Ok(())
}

async fn requeue_stuck_jobs(
    pool: ConnectionPool<Prover>,
    processing_timeout: Duration,
    max_attempts: u32,
) -> anyhow::Result<()> {
    let stuck_jobs = pool
        .connection()
        .await
        .context("failed to build a prover_connection_pool")?
        .fri_witness_generator_dal()
        .requeue_stuck_jobs(processing_timeout, max_attempts)
        .await;

    for stuck_job in stuck_jobs {
        println!("re-queuing fri witness input job {stuck_job:?}");
    }

    Ok(())
}

async fn requeue_successful_jobs(pool: ConnectionPool<Prover>) -> anyhow::Result<()> {
    let stuck_jobs = pool
        .connection()
        .await
        .context("failed to build a prover_connection_pool")?
        .fri_witness_generator_dal()
        .requeue_successful_jobs()
        .await;

    for stuck_job in stuck_jobs {
        println!("re-queuing fri witness input job {stuck_job:?}");
    }
    Ok(())
}

async fn requeue_failed_jobs(
    pool: ConnectionPool<Prover>,
    max_attempts: u32,
) -> anyhow::Result<()> {
    let stuck_jobs = pool
        .connection()
        .await
        .context("failed to build a prover_connection_pool")?
        .fri_witness_generator_dal()
        .requeue_failed_jobs(max_attempts)
        .await;

    for stuck_job in stuck_jobs {
        println!("re-queuing fri witness input job {stuck_job:?}");
    }

    Ok(())
}

async fn requeue_in_progress_jobs(
    pool: ConnectionPool<Prover>,
    processing_timeout: Duration,
    max_attempts: u32,
) -> anyhow::Result<()> {
    let stuck_jobs = pool
        .connection()
        .await
        .context("failed to build a prover_connection_pool")?
        .fri_witness_generator_dal()
        .requeue_in_progress_jobs(processing_timeout, max_attempts)
        .await;

    for stuck_job in stuck_jobs {
        println!("re-queuing fri witness input job {stuck_job:?}");
    }

    Ok(())
}

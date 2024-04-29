use anyhow::Context;
use clap::Args as ClapArgs;
use prover_dal::{Connection, ConnectionPool, Prover, ProverDal};
use zksync_config::PostgresConfig;
use zksync_env_config::FromEnv;
use zksync_types::L1BatchNumber;

#[derive(ClapArgs)]
pub(crate) struct Args {
    #[clap(short, long, conflicts_with = "all")]
    batch: L1BatchNumber,
    #[clap(short, long, conflicts_with = "batch", default_value_t = false)]
    all: bool,
    #[clap(short, long, default_value_t = false)]
    failed: bool,
}

pub(crate) async fn run(args: Args) -> anyhow::Result<()> {
    let config = PostgresConfig::from_env()?;
    let prover_connection_pool =
        ConnectionPool::<Prover>::builder(config.prover_url()?, config.max_connections()?)
            .build()
            .await
            .context("failed to build a prover_connection_pool")?;
    let conn = prover_connection_pool.connection().await.unwrap();

    if args.all {
        delete_prover_db(conn).await?;
    } else {
        delete_batch_data(conn, args.batch, args.failed).await?;
    }

    Ok(())
}

async fn delete_prover_db(mut conn: Connection<'_, Prover>) -> anyhow::Result<()> {
    conn.fri_gpu_prover_queue_dal().delete_all().await;
    conn.fri_prover_jobs_dal().delete_all().await;
    conn.fri_scheduler_dependency_tracker_dal()
        .delete_all()
        .await;
    conn.fri_protocol_versions_dal().delete_all().await;
    conn.fri_proof_compressor_dal().delete_all().await;
    conn.fri_witness_generator_dal().delete_all().await;
    Ok(())
}

async fn delete_batch_data(
    mut conn: Connection<'_, Prover>,
    block_number: L1BatchNumber,
    _failed: bool,
) -> anyhow::Result<()> {
    conn.fri_proof_compressor_dal()
        .delete_batch_data(block_number)
        .await;
    conn.fri_prover_jobs_dal()
        .delete_batch_data(block_number)
        .await;
    conn.fri_witness_generator_dal()
        .delete_batch_data(block_number)
        .await;
    Ok(())
}

use anyhow::Context;
use clap::Args as ClapArgs;
use prover_dal::{
    fri_witness_generator_dal::FriWitnessJobStatus, ConnectionPool, Prover, ProverDal,
};
use zksync_config::PostgresConfig;
use zksync_env_config::FromEnv;
use zksync_types::{basic_fri_types::AggregationRound, L1BatchNumber};

#[derive(ClapArgs)]
pub(crate) struct Args {
    /// Batch number to restart
    #[clap(short, long)]
    batch: L1BatchNumber,
}

pub(crate) async fn run(args: Args) -> anyhow::Result<()> {
    let config = PostgresConfig::from_env()?;
    let prover_connection_pool =
        ConnectionPool::<Prover>::builder(config.prover_url()?, config.max_connections()?)
            .build()
            .await
            .context("failed to build a prover_connection_pool")?;
    let mut conn = prover_connection_pool.connection().await.unwrap();

    conn.fri_proof_compressor_dal()
        .delete_batch_data(args.batch)
        .await
        .context("failed to delete proof compression job for batch")?;
    conn.fri_prover_jobs_dal()
        .delete_batch_data(args.batch)
        .await
        .context("failed to delete prover jobs for batch")?;
    conn.fri_witness_generator_dal()
        .delete_witness_generator_data_for_batch(args.batch, AggregationRound::LeafAggregation)
        .await
        .context("failed to restart batch: fri_witness_generator_dal()")?;
    conn.fri_witness_generator_dal()
        .delete_witness_generator_data_for_batch(args.batch, AggregationRound::NodeAggregation)
        .await
        .context("failed to restart batch: fri_witness_generator_dal()")?;
    conn.fri_witness_generator_dal()
        .delete_witness_generator_data_for_batch(args.batch, AggregationRound::RecursionTip)
        .await
        .context("failed to restart batch: fri_witness_generator_dal()")?;
    conn.fri_witness_generator_dal()
        .delete_witness_generator_data_for_batch(args.batch, AggregationRound::Scheduler)
        .await
        .context("failed to restart batch: fri_witness_generator_dal()")?;
    conn.fri_witness_generator_dal()
        .mark_witness_job(FriWitnessJobStatus::Queued, args.batch)
        .await;

    Ok(())
}

use anyhow::Context;
use clap::Args as ClapArgs;
use zksync_config::configs::DatabaseSecrets;
use zksync_env_config::FromEnv;
use zksync_prover_dal::{
    fri_witness_generator_dal::FriWitnessJobStatus, Connection, ConnectionPool, Prover, ProverDal,
};
use zksync_types::{basic_fri_types::AggregationRound, L1BatchNumber};

#[derive(ClapArgs)]
pub(crate) struct Args {
    /// Batch number to restart
    #[clap(
        short,
        long,
        required_unless_present = "prover_job",
        conflicts_with = "prover_job"
    )]
    batch: Option<L1BatchNumber>,
    /// Prover job to restart
    #[clap(short, long, required_unless_present = "batch")]
    prover_job: Option<u32>,
}

pub(crate) async fn run(args: Args) -> anyhow::Result<()> {
    let config = DatabaseSecrets::from_env()?;
    let prover_connection_pool = ConnectionPool::<Prover>::singleton(config.prover_url()?)
        .build()
        .await
        .context("failed to build a prover_connection_pool")?;
    let mut conn = prover_connection_pool.connection().await.unwrap();

    if let Some(batch_number) = args.batch {
        restart_batch(batch_number, &mut conn).await?;
    } else if let Some(id) = args.prover_job {
        restart_prover_job(id, &mut conn).await;
    }

    Ok(())
}

async fn restart_batch(
    batch_number: L1BatchNumber,
    conn: &mut Connection<'_, Prover>,
) -> anyhow::Result<()> {
    conn.fri_proof_compressor_dal()
        .delete_batch_data(batch_number)
        .await
        .context("failed to delete proof compression job for batch")?;
    conn.fri_prover_jobs_dal()
        .delete_batch_data(batch_number)
        .await
        .context("failed to delete prover jobs for batch")?;
    conn.fri_witness_generator_dal()
        .delete_witness_generator_data_for_batch(batch_number, AggregationRound::LeafAggregation)
        .await
        .context("failed to restart batch: fri_witness_generator_dal()")?;
    conn.fri_witness_generator_dal()
        .delete_witness_generator_data_for_batch(batch_number, AggregationRound::NodeAggregation)
        .await
        .context("failed to restart batch: fri_witness_generator_dal()")?;
    conn.fri_witness_generator_dal()
        .delete_witness_generator_data_for_batch(batch_number, AggregationRound::RecursionTip)
        .await
        .context("failed to restart batch: fri_witness_generator_dal()")?;
    conn.fri_witness_generator_dal()
        .delete_witness_generator_data_for_batch(batch_number, AggregationRound::Scheduler)
        .await
        .context("failed to restart batch: fri_witness_generator_dal()")?;
    conn.fri_witness_generator_dal()
        .mark_witness_job(FriWitnessJobStatus::Queued, batch_number)
        .await;
    Ok(())
}

async fn restart_prover_job(id: u32, conn: &mut Connection<'_, Prover>) {
    conn.fri_prover_jobs_dal().update_status(id, "queued").await;
}

use anyhow::{bail, Context};
use clap::Args as ClapArgs;
use prover_dal::{
    fri_witness_generator_dal::FriWitnessJobStatus, Connection, ConnectionPool, Prover, ProverDal,
};
use zksync_config::PostgresConfig;
use zksync_env_config::FromEnv;
use zksync_types::{basic_fri_types::AggregationRound, L1BatchNumber};

#[derive(ClapArgs)]
pub(crate) struct Args {
    /// Batch number to restart.
    #[clap(short = 'n')]
    batch: L1BatchNumber,
    /// Restart all prover jobs of the batch.
    #[clap(short, long, requires = "batch", conflicts_with_all = ["basic_witness_generator", "leaf_witness_generator", "node_witness_generator", "recursion_tip", "scheduler", "compressor"])]
    prover_jobs: Option<bool>,
    /// Aggregation round from which you want to reset the prover jobs.
    #[clap(short, long, requires = "prover_jobs", conflicts_with_all = ["basic_witness_generator", "leaf_witness_generator", "node_witness_generator", "recursion_tip", "scheduler", "compressor"])]
    aggregation_round: Option<AggregationRound>,
    /// Restart the basic witness generator job of the batch.
    #[clap(long, alias = "bwg", requires = "batch", conflicts_with_all = ["prover", "leaf_witness_generator", "node_witness_generator", "recursion_tip", "scheduler", "compressor"])]
    basic_witness_generator: Option<bool>,
    /// Restart the leaf witness generator jobs of the batch.
    #[clap(long, alias = "lwg", requires = "batch", conflicts_with_all = ["prover", "basic_witness_generator", "node_witness_generator", "recursion_tip", "scheduler", "compressor"])]
    leaf_witness_generator: Option<bool>,
    /// Restart the node witness generator jobs of the batch.
    #[clap(long, alias = "nwg", requires = "batch", conflicts_with_all = ["prover", "basic_witness_generator", "leaf_witness_generator", "recursion_tip", "scheduler", "compressor"])]
    node_witness_generator: Option<bool>,
    /// Restart the recursion tip job of the batch.
    #[clap(long, alias = "rt", requires = "batch", conflicts_with_all = ["prover", "basic_witness_generator", "leaf_witness_generator", "node_witness_generator", "scheduler", "compressor"])]
    recursion_tip: Option<bool>,
    /// Restart the scheduler job of the batch.
    #[clap(short, long, requires = "batch", conflicts_with_all = ["prover", "basic_witness_generator", "leaf_witness_generator", "node_witness_generator", "recursion_tip", "compressor"])]
    scheduler: Option<bool>,
    /// Restart the compressor job of the batch.
    #[clap(short, long, requires = "batch", conflicts_with_all = ["basic_witness_generator", "leaf_witness_generator", "node_witness_generator", "recursion_tip", "scheduler"])]
    compressor: Option<bool>,
}

pub(crate) async fn run(args: Args) -> anyhow::Result<()> {
    let config = PostgresConfig::from_env()?;
    let prover_connection_pool = ConnectionPool::<Prover>::singleton(config.prover_url()?)
        .build()
        .await
        .context("failed to build a prover_connection_pool")?;
    let mut conn = prover_connection_pool.connection().await.unwrap();

    if let Some(true) = args.prover_jobs {
        restart_from_prover_jobs_in_aggregation_round(
            args.aggregation_round
                .context("aggregation round is required")?,
            args.batch,
            &mut conn,
        )
        .await
    } else if let Some(true) = args.basic_witness_generator {
        restart_from_aggregation_round(AggregationRound::BasicCircuits, args.batch, &mut conn).await
    } else if let Some(true) = args.leaf_witness_generator {
        restart_from_aggregation_round(AggregationRound::LeafAggregation, args.batch, &mut conn)
            .await
    } else if let Some(true) = args.node_witness_generator {
        restart_from_aggregation_round(AggregationRound::NodeAggregation, args.batch, &mut conn)
            .await
    } else if let Some(true) = args.recursion_tip {
        restart_from_aggregation_round(AggregationRound::RecursionTip, args.batch, &mut conn).await
    } else if let Some(true) = args.scheduler {
        restart_from_aggregation_round(AggregationRound::Scheduler, args.batch, &mut conn).await
    } else if let Some(true) = args.compressor {
        restart_compressor(args.batch, &mut conn).await
    } else {
        // If no flag was passed, restart from the basic circuits
        restart_from_aggregation_round(AggregationRound::BasicCircuits, args.batch, &mut conn).await
    }
}

async fn restart_from_prover_jobs_in_aggregation_round(
    aggregation_round: AggregationRound,
    batch_number: L1BatchNumber,
    conn: &mut Connection<'_, Prover>,
) -> anyhow::Result<()> {
    match aggregation_round {
        AggregationRound::BasicCircuits => {
            restart_from_aggregation_round(AggregationRound::LeafAggregation, batch_number, conn)
                .await?;
            conn.fri_witness_generator_dal()
                .delete_witness_generator_data_for_batch(
                    batch_number,
                    AggregationRound::LeafAggregation,
                )
                .await
                .context("failed to restart prover jobs in basic witness generation round")?;
        }
        AggregationRound::LeafAggregation => {
            restart_from_aggregation_round(AggregationRound::NodeAggregation, batch_number, conn)
                .await?;
            conn.fri_witness_generator_dal()
                .delete_witness_generator_data_for_batch(
                    batch_number,
                    AggregationRound::NodeAggregation,
                )
                .await
                .context("failed to restart prover jobs in leaf aggregation round")?;
        }
        AggregationRound::NodeAggregation => {
            restart_from_aggregation_round(AggregationRound::RecursionTip, batch_number, conn)
                .await?;
            conn.fri_witness_generator_dal()
                .delete_witness_generator_data_for_batch(
                    batch_number,
                    AggregationRound::RecursionTip,
                )
                .await?;
        }
        AggregationRound::RecursionTip => bail!("recursion tip has no prover jobs"),
        AggregationRound::Scheduler => bail!("scheduler has no prover jobs"),
    };
    Ok(())
}

async fn restart_compressor(
    batch_number: L1BatchNumber,
    conn: &mut Connection<'_, Prover>,
) -> anyhow::Result<()> {
    // Set compressor job to queued
    Ok(())
}

async fn restart_from_aggregation_round(
    aggregation_round: AggregationRound,
    batch_number: L1BatchNumber,
    conn: &mut Connection<'_, Prover>,
) -> anyhow::Result<()> {
    match aggregation_round {
        AggregationRound::BasicCircuits => {
            Box::pin(async {
                restart_from_aggregation_round(
                    AggregationRound::LeafAggregation,
                    batch_number,
                    conn,
                )
                .await
            })
            .await?;
            conn.fri_witness_generator_dal()
                .delete_witness_generator_data_for_batch(
                    batch_number,
                    AggregationRound::LeafAggregation,
                )
                .await
                .context("failed to restart batch: fri_witness_generator_dal()")?;
            conn.fri_prover_jobs_dal()
                .delete_batch_data(batch_number)
                .await
                .context("failed to delete prover jobs for batch")?;
            conn.fri_witness_generator_dal()
                .mark_witness_job(FriWitnessJobStatus::Queued, batch_number)
                .await;
        }
        AggregationRound::LeafAggregation => {
            Box::pin(async {
                restart_from_aggregation_round(
                    AggregationRound::NodeAggregation,
                    batch_number,
                    conn,
                )
                .await
            })
            .await?;
            conn.fri_witness_generator_dal()
                .delete_witness_generator_data_for_batch(
                    batch_number,
                    AggregationRound::NodeAggregation,
                )
                .await
                .context("failed to restart batch: fri_witness_generator_dal()")?;
            conn.fri_prover_jobs_dal()
                .delete_batch_data_for_aggregation_round(batch_number, aggregation_round)
                .await?;
            // Mark leaf aggregation jobs as queued
        }
        AggregationRound::NodeAggregation => {
            Box::pin(async {
                restart_from_aggregation_round(AggregationRound::RecursionTip, batch_number, conn)
                    .await
            })
            .await?;
            conn.fri_witness_generator_dal()
                .delete_witness_generator_data_for_batch(
                    batch_number,
                    AggregationRound::RecursionTip,
                )
                .await
                .context("failed to restart batch: fri_witness_generator_dal()")?;
            conn.fri_prover_jobs_dal()
                .delete_batch_data_for_aggregation_round(batch_number, aggregation_round)
                .await?;
            // Mark node aggregation jobs as queued
        }
        AggregationRound::RecursionTip => {
            Box::pin(async {
                restart_from_aggregation_round(AggregationRound::Scheduler, batch_number, conn)
                    .await
            })
            .await?;
            conn.fri_witness_generator_dal()
                .delete_witness_generator_data_for_batch(batch_number, AggregationRound::Scheduler)
                .await
                .context("failed to restart batch: fri_witness_generator_dal()")?;
            // Mark recursion tip job as queued
        }
        AggregationRound::Scheduler => {
            conn.fri_proof_compressor_dal()
                .delete_batch_data(batch_number)
                .await
                .context("failed to delete proof compression job for batch")?;
            // Mark scheduler job as queued
        }
    }
    Ok(())
}

use anyhow::{ensure, Context as _};
use clap::Args as ClapArgs;
use prover_dal::{Connection, ConnectionPool, Prover, ProverDal};
use zksync_types::{
    basic_fri_types::AggregationRound, prover_dal::WitnessJobStatus, url::SensitiveUrl,
    L1BatchNumber,
};

use super::utils::{AggregationRoundInfo, BatchData, Task, TaskStatus};
use crate::cli::ProverCLIConfig;

#[derive(ClapArgs)]
pub struct Args {
    #[clap(short = 'n', num_args = 1..)]
    batches: Vec<L1BatchNumber>,
    #[clap(short, long, default_value("false"))]
    verbose: bool,
}

pub(crate) async fn run(args: Args, config: ProverCLIConfig) -> anyhow::Result<()> {
    ensure!(
        !args.batches.is_empty(),
        "At least one batch number should be provided"
    );

    let batches_data = get_batches_data(args.batches, config.db_url).await?;

    for batch_data in batches_data {
        println!("{batch_data:?}");
    }

    Ok(())
}

async fn get_batches_data(
    batches: Vec<L1BatchNumber>,
    db_url: SensitiveUrl,
) -> anyhow::Result<Vec<BatchData>> {
    let prover_connection_pool = ConnectionPool::<Prover>::singleton(db_url)
        .build()
        .await
        .context("failed to build a prover_connection_pool")?;

    let mut conn = prover_connection_pool
        .connection()
        .await
        .context("failed to get a connection")?;

    let mut batches_data = Vec::new();
    for batch in batches {
        let current_batch_data = BatchData {
            batch_number: batch,
            basic_witness_generator: Task::BasicWitnessGenerator {
                status: get_proof_basic_witness_generator_status_for_batch(batch, &mut conn).await,
                aggregation_round_info: get_aggregation_round_info_for_batch(
                    batch,
                    AggregationRound::BasicCircuits,
                    &mut conn,
                )
                .await,
            },
            leaf_witness_generator: Task::LeafWitnessGenerator {
                status: get_proof_leaf_witness_generator_status_for_batch(batch, &mut conn).await,
                aggregation_round_info: get_aggregation_round_info_for_batch(
                    batch,
                    AggregationRound::LeafAggregation,
                    &mut conn,
                )
                .await,
            },
            node_witness_generator: Task::NodeWitnessGenerator {
                status: get_proof_node_witness_generator_status_for_batch(batch, &mut conn).await,
                aggregation_round_info: get_aggregation_round_info_for_batch(
                    batch,
                    AggregationRound::NodeAggregation,
                    &mut conn,
                )
                .await,
            },
            scheduler_witness_generator: Task::SchedulerWitnessGenerator {
                status: get_proof_scheduler_witness_generator_status_for_batch(batch, &mut conn)
                    .await,
                aggregation_round_info: get_aggregation_round_info_for_batch(
                    batch,
                    AggregationRound::Scheduler,
                    &mut conn,
                )
                .await,
            },
            compressor: Task::Compressor(
                get_proof_compression_job_status_for_batch(batch, &mut conn).await,
            ),
            ..Default::default()
        };
        batches_data.push(current_batch_data);
    }

    Ok(batches_data)
}

async fn get_aggregation_round_info_for_batch<'a>(
    batch_number: L1BatchNumber,
    aggregation_round: AggregationRound,
    conn: &mut Connection<'a, Prover>,
) -> AggregationRoundInfo {
    let status: TaskStatus = conn
        .fri_prover_jobs_dal()
        .get_prover_jobs_stats_for_batch(batch_number, aggregation_round)
        .await
        .into();

    AggregationRoundInfo {
        round: aggregation_round,
        prover_jobs_status: status,
    }
}

async fn get_proof_basic_witness_generator_status_for_batch<'a>(
    batch_number: L1BatchNumber,
    conn: &mut Connection<'a, Prover>,
) -> TaskStatus {
    conn.fri_witness_generator_dal()
        .get_basic_witness_generator_job_for_batch(batch_number)
        .await
        .map(|job| TaskStatus::from(job.status))
        .unwrap_or_default()
}

async fn get_proof_leaf_witness_generator_status_for_batch<'a>(
    batch_number: L1BatchNumber,
    conn: &mut Connection<'a, Prover>,
) -> TaskStatus {
    conn.fri_witness_generator_dal()
        .get_leaf_witness_generator_jobs_for_batch(batch_number)
        .await
        .iter()
        .map(|s| s.status.clone())
        .collect::<Vec<WitnessJobStatus>>()
        .into()
}

async fn get_proof_node_witness_generator_status_for_batch<'a>(
    batch_number: L1BatchNumber,
    conn: &mut Connection<'a, Prover>,
) -> TaskStatus {
    conn.fri_witness_generator_dal()
        .get_node_witness_generator_jobs_for_batch(batch_number)
        .await
        .iter()
        .map(|s| s.status.clone())
        .collect::<Vec<WitnessJobStatus>>()
        .into()
}

async fn get_proof_scheduler_witness_generator_status_for_batch<'a>(
    batch_number: L1BatchNumber,
    conn: &mut Connection<'a, Prover>,
) -> TaskStatus {
    conn.fri_witness_generator_dal()
        .get_scheduler_witness_generator_jobs_for_batch(batch_number)
        .await
        .iter()
        .map(|s| s.status.clone())
        .collect::<Vec<WitnessJobStatus>>()
        .into()
}

async fn get_proof_compression_job_status_for_batch<'a>(
    batch_number: L1BatchNumber,
    conn: &mut Connection<'a, Prover>,
) -> TaskStatus {
    conn.fri_proof_compressor_dal()
        .get_proof_compression_job_for_batch(batch_number)
        .await
        .map(|job| TaskStatus::from(job.status))
        .unwrap_or_default()
}

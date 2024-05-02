use anyhow::{ensure, Context as _};
use clap::Args as ClapArgs;
use colored::*;
use prover_dal::{Connection, ConnectionPool, Prover, ProverDal};
use zksync_types::{
    basic_fri_types::AggregationRound,
    prover_dal::{
        BasicWitnessGeneratorJobInfo, LeafWitnessGeneratorJobInfo, NodeWitnessGeneratorJobInfo,
        ProofCompressionJobInfo, ProverJobFriInfo, SchedulerWitnessGeneratorJobInfo,
    },
    L1BatchNumber,
};

use super::utils::{BatchData, StageInfo};
use crate::commands::status::utils::{postgres_config, Status};

#[derive(ClapArgs)]
pub struct Args {
    #[clap(short = 'n', num_args = 1..)]
    batches: Vec<L1BatchNumber>,
    #[clap(short, long, default_value("false"))]
    verbose: bool,
}

pub(crate) async fn run(args: Args) -> anyhow::Result<()> {
    ensure!(
        !args.batches.is_empty(),
        "At least one batch number should be provided"
    );

    let batches_data = get_batches_data(args.batches).await?;

    for batch_data in batches_data {
        if !args.verbose {
            display_batch_status(batch_data);
        } else {
            println!("WIP")
        }
    }

    Ok(())
}

async fn get_batches_data(batches: Vec<L1BatchNumber>) -> anyhow::Result<Vec<BatchData>> {
    let config = postgres_config()?;

    let prover_connection_pool =
        ConnectionPool::<Prover>::builder(config.prover_url()?, config.max_connections()?)
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
            basic_witness_generator: StageInfo::BasicWitnessGenerator {
                witness_generator_job_info: get_proof_basic_witness_generator_into_for_batch(
                    batch, &mut conn,
                )
                .await,
                prover_jobs_info: get_prover_jobs_info_for_batch(
                    batch,
                    AggregationRound::BasicCircuits,
                    &mut conn,
                )
                .await,
            },
            leaf_witness_generator: StageInfo::LeafWitnessGenerator {
                witness_generator_jobs_info: get_proof_leaf_witness_generator_info_for_batch(
                    batch, &mut conn,
                )
                .await,
                prover_jobs_info: get_prover_jobs_info_for_batch(
                    batch,
                    AggregationRound::LeafAggregation,
                    &mut conn,
                )
                .await,
            },
            node_witness_generator: StageInfo::NodeWitnessGenerator {
                witness_generator_jobs_info: get_proof_node_witness_generator_info_for_batch(
                    batch, &mut conn,
                )
                .await,
                prover_jobs_info: get_prover_jobs_info_for_batch(
                    batch,
                    AggregationRound::NodeAggregation,
                    &mut conn,
                )
                .await,
            },
            recursion_tip_witness_generator: StageInfo::RecursionTipWitnessGenerator {
                witness_generator_jobs_info: Vec::new(),
                prover_jobs_info: Vec::new(),
            },
            scheduler_witness_generator: StageInfo::SchedulerWitnessGenerator {
                witness_generator_jobs_info: get_proof_scheduler_witness_generator_info_for_batch(
                    batch, &mut conn,
                )
                .await,
                prover_jobs_info: get_prover_jobs_info_for_batch(
                    batch,
                    AggregationRound::Scheduler,
                    &mut conn,
                )
                .await,
            },
            compressor: StageInfo::Compressor(
                get_proof_compression_job_info_for_batch(batch, &mut conn).await,
            ),
        };
        batches_data.push(current_batch_data);
    }

    Ok(batches_data)
}

async fn get_prover_jobs_info_for_batch<'a>(
    batch_number: L1BatchNumber,
    aggregation_round: AggregationRound,
    conn: &mut Connection<'a, Prover>,
) -> Vec<ProverJobFriInfo> {
    conn.fri_prover_jobs_dal()
        .get_prover_jobs_stats_for_batch(batch_number, aggregation_round)
        .await
}

async fn get_proof_basic_witness_generator_into_for_batch<'a>(
    batch_number: L1BatchNumber,
    conn: &mut Connection<'a, Prover>,
) -> Option<BasicWitnessGeneratorJobInfo> {
    conn.fri_witness_generator_dal()
        .get_basic_witness_generator_job_for_batch(batch_number)
        .await
}

async fn get_proof_leaf_witness_generator_info_for_batch<'a>(
    batch_number: L1BatchNumber,
    conn: &mut Connection<'a, Prover>,
) -> Vec<LeafWitnessGeneratorJobInfo> {
    conn.fri_witness_generator_dal()
        .get_leaf_witness_generator_jobs_for_batch(batch_number)
        .await
}

async fn get_proof_node_witness_generator_info_for_batch<'a>(
    batch_number: L1BatchNumber,
    conn: &mut Connection<'a, Prover>,
) -> Vec<NodeWitnessGeneratorJobInfo> {
    conn.fri_witness_generator_dal()
        .get_node_witness_generator_jobs_for_batch(batch_number)
        .await
}

async fn get_proof_scheduler_witness_generator_info_for_batch<'a>(
    batch_number: L1BatchNumber,
    conn: &mut Connection<'a, Prover>,
) -> Vec<SchedulerWitnessGeneratorJobInfo> {
    conn.fri_witness_generator_dal()
        .get_scheduler_witness_generator_jobs_for_batch(batch_number)
        .await
}

async fn get_proof_compression_job_info_for_batch<'a>(
    batch_number: L1BatchNumber,
    conn: &mut Connection<'a, Prover>,
) -> Option<ProofCompressionJobInfo> {
    conn.fri_proof_compressor_dal()
        .get_proof_compression_job_for_batch(batch_number)
        .await
}

fn display_batch_status(batch_data: BatchData) {
    println!(
        "== {} == \n",
        format!("Batch {} Status", batch_data.batch_number)
    );
    display_status_for_stage(batch_data.basic_witness_generator);
    display_status_for_stage(batch_data.leaf_witness_generator);
    display_status_for_stage(batch_data.node_witness_generator);
    display_status_for_stage(batch_data.recursion_tip_witness_generator);
    display_status_for_stage(batch_data.scheduler_witness_generator);
}

fn display_status_for_stage(stage_info: StageInfo) {
    println!(
        "-- {} --",
        format!(
            "Aggregation Round {}",
            stage_info
                .aggregation_round()
                .expect("No aggregation round found") as u8
        )
        .bold()
    );
    match stage_info.witness_generator_jobs_status() {
        Status::Custom(msg) => {
            println!("{}: {}", stage_info.to_string().bold(), msg);
        }
        Status::Queued | Status::WaitingForProofs | Status::Stuck | Status::JobsNotFound => {
            println!(
                "{}: {} \n",
                stage_info.to_string().bold(),
                stage_info.witness_generator_jobs_status()
            )
        }
        Status::InProgress | Status::Successful => {
            println!(
                "{}: {}",
                stage_info.to_string().bold(),
                stage_info.witness_generator_jobs_status()
            );
            println!(
                "> Prover Jobs: {} \n",
                stage_info
                    .prover_jobs_status()
                    .expect("Unable to check status")
            );
        }
    }
}

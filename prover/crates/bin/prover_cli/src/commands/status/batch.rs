use std::collections::BTreeMap;

use anyhow::Context as _;
use circuit_definitions::zkevm_circuits::scheduler::aux::BaseLayerCircuitType;
use clap::Args as ClapArgs;
use colored::*;
use zksync_prover_dal::{Connection, ConnectionPool, Prover, ProverDal};
use zksync_types::{
    basic_fri_types::AggregationRound,
    prover_dal::{
        BasicWitnessGeneratorJobInfo, ExtendedJobCountStatistics, LeafWitnessGeneratorJobInfo,
        NodeWitnessGeneratorJobInfo, ProofCompressionJobInfo, ProverJobFriInfo, ProverJobStatus,
        RecursionTipWitnessGeneratorJobInfo, SchedulerWitnessGeneratorJobInfo,
    },
    url::SensitiveUrl,
    L1BatchNumber,
};

use super::utils::{BatchData, StageInfo, Status};
use crate::cli::ProverCLIConfig;

#[derive(ClapArgs)]
pub struct Args {
    #[clap(short = 'n', num_args = 1.., required = true)]
    batches: Vec<L1BatchNumber>,
    #[clap(short, long, default_value("false"))]
    verbose: bool,
}

pub(crate) async fn run(args: Args, config: ProverCLIConfig) -> anyhow::Result<()> {
    let batches_data = get_batches_data(args.batches, config.db_url).await?;

    for batch_data in batches_data {
        println!(
            "== {} ==",
            format!("Batch {} Status", batch_data.batch_number).bold()
        );

        if let Status::Custom(msg) = batch_data.compressor.witness_generator_jobs_status() {
            if msg.contains("Sent to server") {
                println!("> Proof sent to server ✅");
                continue;
            }
        }

        let basic_witness_generator_status = batch_data
            .basic_witness_generator
            .witness_generator_jobs_status();
        if matches!(basic_witness_generator_status, Status::JobsNotFound) {
            println!("> No batch found. 🚫");
            continue;
        }

        if !args.verbose {
            display_batch_status(batch_data);
        } else {
            display_batch_info(batch_data);
        }
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
            recursion_tip_witness_generator: StageInfo::RecursionTipWitnessGenerator(
                get_proof_recursion_tip_witness_generator_info_for_batch(batch, &mut conn).await,
            ),
            scheduler_witness_generator: StageInfo::SchedulerWitnessGenerator(
                get_proof_scheduler_witness_generator_info_for_batch(batch, &mut conn).await,
            ),
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

async fn get_proof_recursion_tip_witness_generator_info_for_batch<'a>(
    batch_number: L1BatchNumber,
    conn: &mut Connection<'a, Prover>,
) -> Option<RecursionTipWitnessGeneratorJobInfo> {
    conn.fri_witness_generator_dal()
        .get_recursion_tip_witness_generator_jobs_for_batch(batch_number)
        .await
}

async fn get_proof_scheduler_witness_generator_info_for_batch<'a>(
    batch_number: L1BatchNumber,
    conn: &mut Connection<'a, Prover>,
) -> Option<SchedulerWitnessGeneratorJobInfo> {
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
    display_status_for_stage(batch_data.basic_witness_generator);
    display_status_for_stage(batch_data.leaf_witness_generator);
    display_status_for_stage(batch_data.node_witness_generator);
    display_status_for_stage(batch_data.recursion_tip_witness_generator);
    display_status_for_stage(batch_data.scheduler_witness_generator);
    display_status_for_stage(batch_data.compressor);
}

fn display_status_for_stage(stage_info: StageInfo) {
    display_aggregation_round(&stage_info);
    match stage_info.witness_generator_jobs_status() {
        Status::Custom(msg) => {
            println!("{}: {} \n", stage_info.to_string().bold(), msg);
        }
        Status::Queued | Status::WaitingForProofs | Status::Stuck | Status::JobsNotFound => {
            println!(
                "{}: {}",
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
            if let Some(job_status) = stage_info.prover_jobs_status() {
                println!("> {}: {}", "Prover Jobs".to_owned().bold(), job_status);
            }
        }
    }
}

fn display_batch_info(batch_data: BatchData) {
    display_info_for_stage(batch_data.basic_witness_generator);
    display_info_for_stage(batch_data.leaf_witness_generator);
    display_info_for_stage(batch_data.node_witness_generator);
    display_info_for_stage(batch_data.recursion_tip_witness_generator);
    display_info_for_stage(batch_data.scheduler_witness_generator);
    display_info_for_stage(batch_data.compressor);
}

fn display_info_for_stage(stage_info: StageInfo) {
    display_aggregation_round(&stage_info);
    match stage_info.witness_generator_jobs_status() {
        Status::Custom(msg) => {
            println!("{}: {}", stage_info.to_string().bold(), msg);
        }
        Status::Queued | Status::WaitingForProofs | Status::Stuck | Status::JobsNotFound => {
            println!(
                " > {}: {}",
                stage_info.to_string().bold(),
                stage_info.witness_generator_jobs_status()
            )
        }
        Status::InProgress => {
            println!(
                "v {}: {}",
                stage_info.to_string().bold(),
                stage_info.witness_generator_jobs_status()
            );
            match stage_info {
                StageInfo::BasicWitnessGenerator {
                    prover_jobs_info, ..
                } => {
                    display_prover_jobs_info(prover_jobs_info);
                }
                StageInfo::LeafWitnessGenerator {
                    witness_generator_jobs_info,
                    prover_jobs_info,
                } => {
                    display_leaf_witness_generator_jobs_info(witness_generator_jobs_info);
                    display_prover_jobs_info(prover_jobs_info);
                }
                StageInfo::NodeWitnessGenerator {
                    witness_generator_jobs_info,
                    prover_jobs_info,
                } => {
                    display_node_witness_generator_jobs_info(witness_generator_jobs_info);
                    display_prover_jobs_info(prover_jobs_info);
                }
                _ => (),
            }
        }
        Status::Successful => {
            println!(
                "> {}: {}",
                stage_info.to_string().bold(),
                stage_info.witness_generator_jobs_status()
            );
            match stage_info {
                StageInfo::BasicWitnessGenerator {
                    prover_jobs_info, ..
                }
                | StageInfo::LeafWitnessGenerator {
                    prover_jobs_info, ..
                }
                | StageInfo::NodeWitnessGenerator {
                    prover_jobs_info, ..
                } => display_prover_jobs_info(prover_jobs_info),
                _ => (),
            }
        }
    }
}

fn display_leaf_witness_generator_jobs_info(
    mut leaf_witness_generators_jobs_info: Vec<LeafWitnessGeneratorJobInfo>,
) {
    leaf_witness_generators_jobs_info.sort_by_key(|job| job.circuit_id);

    leaf_witness_generators_jobs_info.iter().for_each(|job| {
        println!(
            "   > {}: {}",
            format!(
                "{:?}",
                BaseLayerCircuitType::from_numeric_value(job.circuit_id as u8)
            )
            .bold(),
            Status::from(job.status.clone())
        )
    });
}

fn display_node_witness_generator_jobs_info(
    mut node_witness_generators_jobs_info: Vec<NodeWitnessGeneratorJobInfo>,
) {
    node_witness_generators_jobs_info.sort_by_key(|job| job.circuit_id);

    node_witness_generators_jobs_info.iter().for_each(|job| {
        println!(
            "   > {}: {}",
            format!(
                "{:?}",
                BaseLayerCircuitType::from_numeric_value(job.circuit_id as u8)
            )
            .bold(),
            Status::from(job.status.clone())
        )
    });
}

fn display_prover_jobs_info(prover_jobs_info: Vec<ProverJobFriInfo>) {
    let prover_jobs_status = Status::from(prover_jobs_info.clone());

    if matches!(prover_jobs_status, Status::Successful)
        || matches!(prover_jobs_status, Status::JobsNotFound)
    {
        println!(
            "> {}: {prover_jobs_status}",
            "Prover Jobs".to_owned().bold()
        );
        return;
    }

    println!(
        "v {}: {prover_jobs_status}",
        "Prover Jobs".to_owned().bold()
    );

    let mut jobs_by_circuit_id: BTreeMap<u32, Vec<ProverJobFriInfo>> = BTreeMap::new();
    prover_jobs_info.iter().for_each(|job| {
        jobs_by_circuit_id
            .entry(job.circuit_id)
            .or_default()
            .push(job.clone())
    });

    for (circuit_id, prover_jobs_info) in jobs_by_circuit_id {
        let status = Status::from(prover_jobs_info.clone());
        println!(
            "   > {}: {}",
            format!(
                "{:?}",
                BaseLayerCircuitType::from_numeric_value(circuit_id as u8)
            )
            .bold(),
            status
        );
        if matches!(status, Status::InProgress) {
            display_job_status_count(prover_jobs_info);
        }
    }
}

fn display_job_status_count(jobs: Vec<ProverJobFriInfo>) {
    let mut jobs_counts = ExtendedJobCountStatistics::default();
    let total_jobs = jobs.len();
    jobs.iter().for_each(|job| match job.status {
        ProverJobStatus::Queued => jobs_counts.queued += 1,
        ProverJobStatus::InProgress(_) => jobs_counts.in_progress += 1,
        ProverJobStatus::Successful(_) => jobs_counts.successful += 1,
        ProverJobStatus::Failed(_) => jobs_counts.failed += 1,
        ProverJobStatus::Skipped | ProverJobStatus::Ignored | ProverJobStatus::InGPUProof => (),
    });

    println!("     - Total jobs: {}", total_jobs);
    println!("     - Successful: {}", jobs_counts.successful);
    println!("     - In Progress: {}", jobs_counts.in_progress);
    println!("     - Queued: {}", jobs_counts.queued);
    println!("     - Failed: {}", jobs_counts.failed);
}

fn display_aggregation_round(stage_info: &StageInfo) {
    if let Some(aggregation_round) = stage_info.aggregation_round() {
        println!(
            "\n-- {} --",
            format!("Aggregation Round {}", aggregation_round as u8).bold()
        );
    } else {
        println!("\n-- {} --", "Proof Compression".to_owned().bold());
    };
}

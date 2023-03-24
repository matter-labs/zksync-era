use std::fmt::Debug;
use std::time::Instant;

use async_trait::async_trait;
use rand::Rng;

use zksync_config::configs::WitnessGeneratorConfig;
use zksync_dal::ConnectionPool;
use zksync_object_store::object_store::create_object_store_from_env;
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{
    proofs::{AggregationRound, WitnessGeneratorJob, WitnessGeneratorJobInput},
    zkevm_test_harness::{
        abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit, bellman::bn256::Bn256,
        witness::oracle::VmWitnessOracle,
    },
    L1BatchNumber,
};

// use crate::witness_generator::basic_circuits;
use crate::witness_generator::basic_circuits::BasicCircuitArtifacts;
use crate::witness_generator::leaf_aggregation::LeafAggregationArtifacts;
use crate::witness_generator::node_aggregation::NodeAggregationArtifacts;
use crate::witness_generator::scheduler::SchedulerArtifacts;

mod precalculated_merkle_paths_provider;
mod utils;

mod basic_circuits;
mod leaf_aggregation;
mod node_aggregation;
mod scheduler;
#[cfg(test)]
mod tests;

/// `WitnessGenerator` component is responsible for generating prover jobs
/// and saving artifacts needed for the next round of proof aggregation.
///
/// That is, every aggregation round needs two sets of input:
///  * computed proofs from the previous round
///  * some artifacts that the witness generator of previous round(s) returns.
///
/// There are four rounds of proofs for every block,
/// each of them starts with an invocation of `WitnessGenerator` with a corresponding `WitnessGeneratorJobType`:
///  * `WitnessGeneratorJobType::BasicCircuits`:
///         generates basic circuits (circuits like `Main VM` - up to 50 * 48 = 2400 circuits):
///         input table: `basic_circuit_witness_jobs`
///         artifact/output table: `leaf_aggregation_jobs` (also creates job stubs in `node_aggregation_jobs` and `scheduler_aggregation_jobs`)
///         value in `aggregation_round` field of `prover_jobs` table: 0
///  * `WitnessGeneratorJobType::LeafAggregation`:
///         generates leaf aggregation circuits (up to 48 circuits of type `LeafAggregation`)
///         input table: `leaf_aggregation_jobs`
///         artifact/output table: `node_aggregation_jobs`
///         value in `aggregation_round` field of `prover_jobs` table: 1
///  * `WitnessGeneratorJobType::NodeAggregation`
///         generates one circuit of type `NodeAggregation`
///         input table: `leaf_aggregation_jobs`
///         value in `aggregation_round` field of `prover_jobs` table: 2
///  * scheduler circuit
///         generates one circuit of type `Scheduler`
///         input table: `scheduler_witness_jobs`
///         value in `aggregation_round` field of `prover_jobs` table: 3
///
/// One round of prover generation consists of:
///  * `WitnessGenerator` picks up the next `queued` job in its input table and processes it
///         (invoking the corresponding helper function in `zkevm_test_harness` repo)
///  * it saves the generated circuis to `prover_jobs` table and the other artifacts to its output table
///  * the individual proofs are picked up by the provers, processed, and marked as complete.
///  * when the last proof for this round is computed, the prover updates the row in the output table
///    setting its status to `queued`
///  * `WitnessGenerator` picks up such job and proceeds to the next round
///
/// Note that the very first input table (`basic_circuit_witness_jobs`)
/// is populated by the tree (as the input artifact for the `WitnessGeneratorJobType::BasicCircuits` is the merkle proofs)
///
#[derive(Debug)]
pub struct WitnessGenerator {
    config: WitnessGeneratorConfig,
}

pub enum WitnessGeneratorArtifacts {
    BasicCircuits(Box<BasicCircuitArtifacts>),
    LeafAggregation(Box<LeafAggregationArtifacts>),
    NodeAggregation(Box<NodeAggregationArtifacts>),
    Scheduler(Box<SchedulerArtifacts>),
}

impl WitnessGenerator {
    pub fn new(config: WitnessGeneratorConfig) -> Self {
        Self { config }
    }

    fn process_job_sync(
        connection_pool: ConnectionPool,
        job: WitnessGeneratorJob,
        started_at: Instant,
    ) -> Option<WitnessGeneratorArtifacts> {
        let config: WitnessGeneratorConfig = WitnessGeneratorConfig::from_env();
        let WitnessGeneratorJob { block_number, job } = job;

        if let (Some(blocks_proving_percentage), &WitnessGeneratorJobInput::BasicCircuits(_)) =
            (config.blocks_proving_percentage, &job)
        {
            // Generate random number in (0; 100).
            let rand_value = rand::thread_rng().gen_range(1..100);
            // We get value higher than `blocks_proving_percentage` with prob = `1 - blocks_proving_percentage`.
            // In this case job should be skipped.
            if rand_value > blocks_proving_percentage {
                metrics::counter!("server.witness_generator.skipped_blocks", 1);
                vlog::info!(
                    "Skipping witness generation for block {}, blocks_proving_percentage: {}",
                    block_number.0,
                    blocks_proving_percentage
                );
                let mut storage = connection_pool.access_storage_blocking();
                storage
                    .witness_generator_dal()
                    .mark_witness_job_as_skipped(block_number, AggregationRound::BasicCircuits);
                return None;
            }
        }

        if matches!(&job, &WitnessGeneratorJobInput::BasicCircuits(_)) {
            metrics::counter!("server.witness_generator.sampled_blocks", 1);
        }
        vlog::info!(
            "Starting witness generation of type {:?} for block {}",
            job.aggregation_round(),
            block_number.0
        );

        match job {
            WitnessGeneratorJobInput::BasicCircuits(job) => {
                Some(WitnessGeneratorArtifacts::BasicCircuits(Box::new(
                    basic_circuits::process_basic_circuits_job(
                        config,
                        connection_pool,
                        started_at,
                        block_number,
                        *job,
                    ),
                )))
            }
            WitnessGeneratorJobInput::LeafAggregation(job) => {
                Some(WitnessGeneratorArtifacts::LeafAggregation(Box::new(
                    leaf_aggregation::process_leaf_aggregation_job(started_at, block_number, *job),
                )))
            }
            WitnessGeneratorJobInput::NodeAggregation(job) => {
                Some(WitnessGeneratorArtifacts::NodeAggregation(Box::new(
                    node_aggregation::process_node_aggregation_job(
                        config,
                        started_at,
                        block_number,
                        *job,
                    ),
                )))
            }

            WitnessGeneratorJobInput::Scheduler(job) => {
                Some(WitnessGeneratorArtifacts::Scheduler(Box::new(
                    scheduler::process_scheduler_job(started_at, block_number, *job),
                )))
            }
        }
    }
}

#[async_trait]
impl JobProcessor for WitnessGenerator {
    type Job = WitnessGeneratorJob;
    type JobId = (L1BatchNumber, AggregationRound);
    type JobArtifacts = Option<WitnessGeneratorArtifacts>;

    const SERVICE_NAME: &'static str = "witness_generator";

    async fn get_next_job(
        &self,
        connection_pool: ConnectionPool,
    ) -> Option<(Self::JobId, Self::Job)> {
        let mut connection = connection_pool.access_storage().await;
        let object_store = create_object_store_from_env();
        let last_l1_batch_to_process = self.config.last_l1_batch_to_process();
        let optional_metadata = connection
            .witness_generator_dal()
            .get_next_scheduler_witness_job(
                self.config.witness_generation_timeout(),
                self.config.max_attempts,
                last_l1_batch_to_process,
            );

        if let Some(metadata) = optional_metadata {
            let prev_metadata = connection
                .blocks_dal()
                .get_block_metadata(metadata.block_number - 1);
            let previous_aux_hash = prev_metadata
                .as_ref()
                .map_or([0u8; 32], |e| e.metadata.aux_data_hash.0);
            let previous_meta_hash =
                prev_metadata.map_or([0u8; 32], |e| e.metadata.meta_parameters_hash.0);
            let job = scheduler::get_artifacts(
                metadata,
                previous_aux_hash,
                previous_meta_hash,
                &object_store,
            )
            .await;
            return Some(((job.block_number, job.job.aggregation_round()), job));
        }

        let optional_metadata = connection
            .witness_generator_dal()
            .get_next_node_aggregation_witness_job(
                self.config.witness_generation_timeout(),
                self.config.max_attempts,
                last_l1_batch_to_process,
            );

        if let Some(metadata) = optional_metadata {
            let job = node_aggregation::get_artifacts(metadata, &object_store).await;
            return Some(((job.block_number, job.job.aggregation_round()), job));
        }

        let optional_metadata = connection
            .witness_generator_dal()
            .get_next_leaf_aggregation_witness_job(
                self.config.witness_generation_timeout(),
                self.config.max_attempts,
                last_l1_batch_to_process,
            );

        if let Some(metadata) = optional_metadata {
            let job = leaf_aggregation::get_artifacts(metadata, &object_store).await;
            return Some(((job.block_number, job.job.aggregation_round()), job));
        }

        let optional_metadata = connection
            .witness_generator_dal()
            .get_next_basic_circuit_witness_job(
                self.config.witness_generation_timeout(),
                self.config.max_attempts,
                last_l1_batch_to_process,
            );

        if let Some(metadata) = optional_metadata {
            let job = basic_circuits::get_artifacts(metadata.block_number, &object_store).await;
            return Some(((job.block_number, job.job.aggregation_round()), job));
        }

        None
    }

    async fn save_failure(
        connection_pool: ConnectionPool,
        job_id: (L1BatchNumber, AggregationRound),
        started_at: Instant,
        error: String,
    ) -> () {
        let config: WitnessGeneratorConfig = WitnessGeneratorConfig::from_env();
        connection_pool
            .access_storage_blocking()
            .witness_generator_dal()
            .mark_witness_job_as_failed(
                job_id.0,
                job_id.1,
                started_at.elapsed(),
                error,
                config.max_attempts,
            );
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        connection_pool: ConnectionPool,
        job: WitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<Option<WitnessGeneratorArtifacts>> {
        tokio::task::spawn_blocking(move || {
            Self::process_job_sync(connection_pool.clone(), job, started_at)
        })
    }

    async fn save_result(
        connection_pool: ConnectionPool,
        job_id: (L1BatchNumber, AggregationRound),
        started_at: Instant,
        optional_artifacts: Option<WitnessGeneratorArtifacts>,
    ) {
        match optional_artifacts {
            None => (),
            Some(artifacts) => {
                let mut object_store = create_object_store_from_env();
                let block_number = job_id.0;
                match artifacts {
                    WitnessGeneratorArtifacts::BasicCircuits(boxed_basic_circuit_artifacts) => {
                        let basic_circuit_artifacts = *boxed_basic_circuit_artifacts;
                        let circuits =
                            get_circuit_types(&basic_circuit_artifacts.serialized_circuits);
                        basic_circuits::save_artifacts(
                            block_number,
                            basic_circuit_artifacts,
                            &mut object_store,
                        )
                        .await;
                        basic_circuits::update_database(
                            connection_pool,
                            started_at,
                            block_number,
                            circuits,
                        );
                    }
                    WitnessGeneratorArtifacts::LeafAggregation(
                        boxed_leaf_aggregation_artifacts,
                    ) => {
                        let leaf_aggregation_artifacts = *boxed_leaf_aggregation_artifacts;
                        let leaf_circuits_len = leaf_aggregation_artifacts.leaf_circuits.len();
                        let circuits =
                            get_circuit_types(&leaf_aggregation_artifacts.serialized_circuits);
                        leaf_aggregation::save_artifacts(
                            block_number,
                            leaf_aggregation_artifacts,
                            &mut object_store,
                        )
                        .await;
                        leaf_aggregation::update_database(
                            connection_pool,
                            started_at,
                            block_number,
                            leaf_circuits_len,
                            circuits,
                        );
                    }
                    WitnessGeneratorArtifacts::NodeAggregation(
                        boxed_node_aggregation_artifacts,
                    ) => {
                        let node_aggregation_artifacts = *boxed_node_aggregation_artifacts;
                        let circuits =
                            get_circuit_types(&node_aggregation_artifacts.serialized_circuits);
                        node_aggregation::save_artifacts(
                            block_number,
                            node_aggregation_artifacts,
                            &mut object_store,
                        )
                        .await;
                        node_aggregation::update_database(
                            connection_pool,
                            started_at,
                            block_number,
                            circuits,
                        );
                    }
                    WitnessGeneratorArtifacts::Scheduler(boxed_scheduler_artifacts) => {
                        let scheduler_artifacts = *boxed_scheduler_artifacts;
                        let circuits = get_circuit_types(&scheduler_artifacts.serialized_circuits);
                        scheduler::save_artifacts(
                            block_number,
                            scheduler_artifacts.serialized_circuits,
                            &mut object_store,
                        )
                        .await;
                        scheduler::update_database(
                            connection_pool,
                            started_at,
                            block_number,
                            scheduler_artifacts.final_aggregation_result,
                            circuits,
                        );
                    }
                };
            }
        }
    }
}

fn get_circuit_types(serialized_circuits: &[(String, Vec<u8>)]) -> Vec<String> {
    serialized_circuits
        .iter()
        .map(|(circuit, _)| circuit.clone())
        .collect()
}

fn track_witness_generation_stage(started_at: Instant, round: AggregationRound) {
    let stage = match round {
        AggregationRound::BasicCircuits => "basic_circuits",
        AggregationRound::LeafAggregation => "leaf_aggregation",
        AggregationRound::NodeAggregation => "node_aggregation",
        AggregationRound::Scheduler => "scheduler",
    };
    metrics::histogram!(
        "server.witness_generator.processing_time",
        started_at.elapsed(),
        "stage" => format!("wit_gen_{}", stage)
    );
}

fn serialize_circuits(
    individual_circuits: &[ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>],
) -> Vec<(String, Vec<u8>)> {
    individual_circuits
        .iter()
        .map(|circuit| {
            (
                circuit.short_description().to_owned(),
                bincode::serialize(&circuit).expect("failed to serialize circuit"),
            )
        })
        .collect()
}

const _SCHEDULER_CIRCUIT_INDEX: u8 = 0;

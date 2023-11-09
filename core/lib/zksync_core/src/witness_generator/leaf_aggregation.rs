use async_trait::async_trait;

use std::{collections::HashMap, time::Instant};

use zksync_config::configs::WitnessGeneratorConfig;
use zksync_dal::ConnectionPool;
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{
    circuit::LEAF_SPLITTING_FACTOR,
    proofs::{AggregationRound, PrepareLeafAggregationCircuitsJob, WitnessGeneratorJobMetadata},
    zkevm_test_harness::{
        abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit, bellman::bn256::Bn256,
        bellman::plonk::better_better_cs::setup::VerificationKey,
        encodings::recursion_request::RecursionRequest, encodings::QueueSimulator, witness,
        witness::oracle::VmWitnessOracle, LeafAggregationOutputDataWitness,
    },
    L1BatchNumber, ProtocolVersionId,
};
use zksync_verification_key_server::{
    get_ordered_vks_for_basic_circuits, get_vks_for_basic_circuits, get_vks_for_commitment,
};

use super::{utils::save_prover_input_artifacts, METRICS};

pub struct LeafAggregationArtifacts {
    leaf_layer_subqueues: Vec<QueueSimulator<Bn256, RecursionRequest<Bn256>, 2, 2>>,
    aggregation_outputs: Vec<LeafAggregationOutputDataWitness<Bn256>>,
    leaf_circuits: Vec<ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
}

#[derive(Debug)]
struct BlobUrls {
    leaf_layer_subqueues_url: String,
    aggregation_outputs_url: String,
    circuit_types_and_urls: Vec<(&'static str, String)>,
}

#[derive(Clone)]
pub struct LeafAggregationWitnessGeneratorJob {
    block_number: L1BatchNumber,
    job: PrepareLeafAggregationCircuitsJob,
}

#[derive(Debug)]
pub struct LeafAggregationWitnessGenerator {
    config: WitnessGeneratorConfig,
    object_store: Box<dyn ObjectStore>,
    protocol_versions: Vec<ProtocolVersionId>,
    connection_pool: ConnectionPool,
    prover_connection_pool: ConnectionPool,
}

impl LeafAggregationWitnessGenerator {
    pub async fn new(
        config: WitnessGeneratorConfig,
        store_factory: &ObjectStoreFactory,
        protocol_versions: Vec<ProtocolVersionId>,
        connection_pool: ConnectionPool,
        prover_connection_pool: ConnectionPool,
    ) -> Self {
        Self {
            config,
            object_store: store_factory.create_store().await,
            protocol_versions,
            connection_pool,
            prover_connection_pool,
        }
    }

    fn process_job_sync(
        leaf_job: LeafAggregationWitnessGeneratorJob,
        started_at: Instant,
    ) -> LeafAggregationArtifacts {
        let LeafAggregationWitnessGeneratorJob { block_number, job } = leaf_job;

        tracing::info!(
            "Starting witness generation of type {:?} for block {}",
            AggregationRound::LeafAggregation,
            block_number.0
        );
        process_leaf_aggregation_job(started_at, block_number, job)
    }
}

#[async_trait]
impl JobProcessor for LeafAggregationWitnessGenerator {
    type Job = LeafAggregationWitnessGeneratorJob;
    type JobId = L1BatchNumber;
    type JobArtifacts = LeafAggregationArtifacts;

    const SERVICE_NAME: &'static str = "leaf_aggregation_witness_generator";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut prover_connection = self.prover_connection_pool.access_storage().await.unwrap();
        let last_l1_batch_to_process = self.config.last_l1_batch_to_process();

        Ok(
            match prover_connection
                .witness_generator_dal()
                .get_next_leaf_aggregation_witness_job(
                    self.config.witness_generation_timeout(),
                    self.config.max_attempts,
                    last_l1_batch_to_process,
                    &self.protocol_versions,
                )
                .await
            {
                Some(metadata) => {
                    let job = get_artifacts(metadata, &*self.object_store).await;
                    Some((job.block_number, job))
                }
                None => None,
            },
        )
    }

    async fn save_failure(&self, job_id: L1BatchNumber, started_at: Instant, error: String) -> () {
        let attempts = self
            .prover_connection_pool
            .access_storage()
            .await
            .unwrap()
            .witness_generator_dal()
            .mark_witness_job_as_failed(
                AggregationRound::LeafAggregation,
                job_id,
                started_at.elapsed(),
                error,
            )
            .await;

        if attempts >= self.config.max_attempts {
            self.connection_pool
                .access_storage()
                .await
                .unwrap()
                .blocks_dal()
                .set_skip_proof_for_l1_batch(job_id)
                .await
                .unwrap();
        }
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        job: LeafAggregationWitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<anyhow::Result<LeafAggregationArtifacts>> {
        tokio::task::spawn_blocking(move || Ok(Self::process_job_sync(job, started_at)))
    }

    async fn save_result(
        &self,
        job_id: L1BatchNumber,
        started_at: Instant,
        artifacts: LeafAggregationArtifacts,
    ) -> anyhow::Result<()> {
        let leaf_circuits_len = artifacts.leaf_circuits.len();
        let blob_urls = save_artifacts(job_id, artifacts, &*self.object_store).await;
        update_database(
            &self.prover_connection_pool,
            started_at,
            job_id,
            leaf_circuits_len,
            blob_urls,
        )
        .await;
        Ok(())
    }

    fn max_attempts(&self) -> u32 {
        self.config.max_attempts
    }

    async fn get_job_attempts(&self, _job_id: &Self::JobId) -> anyhow::Result<u32> {
        // Witness generator will be removed soon in favor of FRI one, so returning blank value.
        Ok(1)
    }
}

pub fn process_leaf_aggregation_job(
    started_at: Instant,
    block_number: L1BatchNumber,
    job: PrepareLeafAggregationCircuitsJob,
) -> LeafAggregationArtifacts {
    let stage_started_at = Instant::now();

    let verification_keys: HashMap<
        u8,
        VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
    > = get_vks_for_basic_circuits();

    tracing::info!(
        "Verification keys loaded in {:?}",
        stage_started_at.elapsed()
    );

    // we need the list of vks that matches the list of job.basic_circuit_proofs
    let vks_for_aggregation: Vec<
        VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
    > = get_ordered_vks_for_basic_circuits(&job.basic_circuits, &verification_keys);

    let (all_vk_committments, set_committment, g2_points) =
        witness::recursive_aggregation::form_base_circuits_committment(get_vks_for_commitment(
            verification_keys,
        ));

    tracing::info!("Commitments generated in {:?}", stage_started_at.elapsed());

    let stage_started_at = Instant::now();

    let (leaf_layer_subqueues, aggregation_outputs, leaf_circuits) =
        witness::recursive_aggregation::prepare_leaf_aggregations(
            job.basic_circuits,
            job.basic_circuits_inputs,
            job.basic_circuits_proofs,
            vks_for_aggregation,
            LEAF_SPLITTING_FACTOR,
            all_vk_committments,
            set_committment,
            g2_points,
        );

    tracing::info!(
        "prepare_leaf_aggregations took {:?}",
        stage_started_at.elapsed()
    );
    tracing::info!(
        "Leaf witness generation for block {} is complete in {:?}. Number of circuits: {}",
        block_number.0,
        started_at.elapsed(),
        leaf_circuits.len()
    );

    LeafAggregationArtifacts {
        leaf_layer_subqueues,
        aggregation_outputs,
        leaf_circuits,
    }
}

async fn update_database(
    prover_connection_pool: &ConnectionPool,
    started_at: Instant,
    block_number: L1BatchNumber,
    leaf_circuits_len: usize,
    blob_urls: BlobUrls,
) {
    let mut prover_connection = prover_connection_pool.access_storage().await.unwrap();
    let mut transaction = prover_connection.start_transaction().await.unwrap();

    // inserts artifacts into the node_aggregation_witness_jobs table
    // and advances it to waiting_for_proofs status
    transaction
        .witness_generator_dal()
        .save_leaf_aggregation_artifacts(
            block_number,
            leaf_circuits_len,
            &blob_urls.leaf_layer_subqueues_url,
            &blob_urls.aggregation_outputs_url,
        )
        .await;
    let system_version = transaction
        .witness_generator_dal()
        .protocol_version_for_l1_batch(block_number)
        .await
        .unwrap_or_else(|| {
            panic!(
                "No system version exist for l1 batch {} for leaf agg",
                block_number.0
            )
        });
    transaction
        .prover_dal()
        .insert_prover_jobs(
            block_number,
            blob_urls.circuit_types_and_urls,
            AggregationRound::LeafAggregation,
            system_version,
        )
        .await;
    transaction
        .witness_generator_dal()
        .mark_witness_job_as_successful(
            block_number,
            AggregationRound::LeafAggregation,
            started_at.elapsed(),
        )
        .await;

    transaction.commit().await.unwrap();
    METRICS.processing_time[&AggregationRound::LeafAggregation.into()]
        .observe(started_at.elapsed());
}

async fn get_artifacts(
    metadata: WitnessGeneratorJobMetadata,
    object_store: &dyn ObjectStore,
) -> LeafAggregationWitnessGeneratorJob {
    let basic_circuits = object_store.get(metadata.block_number).await.unwrap();
    let basic_circuits_inputs = object_store.get(metadata.block_number).await.unwrap();

    LeafAggregationWitnessGeneratorJob {
        block_number: metadata.block_number,
        job: PrepareLeafAggregationCircuitsJob {
            basic_circuits_inputs,
            basic_circuits_proofs: metadata.proofs,
            basic_circuits,
        },
    }
}

async fn save_artifacts(
    block_number: L1BatchNumber,
    artifacts: LeafAggregationArtifacts,
    object_store: &dyn ObjectStore,
) -> BlobUrls {
    let leaf_layer_subqueues_url = object_store
        .put(block_number, &artifacts.leaf_layer_subqueues)
        .await
        .unwrap();
    let aggregation_outputs_url = object_store
        .put(block_number, &artifacts.aggregation_outputs)
        .await
        .unwrap();
    let circuit_types_and_urls = save_prover_input_artifacts(
        block_number,
        &artifacts.leaf_circuits,
        object_store,
        AggregationRound::LeafAggregation,
    )
    .await;
    BlobUrls {
        leaf_layer_subqueues_url,
        aggregation_outputs_url,
        circuit_types_and_urls,
    }
}

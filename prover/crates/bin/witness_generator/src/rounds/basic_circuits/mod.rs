use std::{sync::Arc, time::Instant};

use async_trait::async_trait;
use circuit_definitions::zkevm_circuits::scheduler::{
    block_header::BlockAuxilaryOutputWitness, input::SchedulerCircuitInstanceWitness,
};
use zksync_config::configs::FriWitnessGeneratorConfig;
use zksync_multivm::circuit_sequencer_api_latest::boojum::{
    field::goldilocks::{GoldilocksExt2, GoldilocksField},
    gadgets::recursion::recursive_tree_hasher::CircuitGoldilocksPoseidon2Sponge,
};
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_interface::inputs::WitnessInputData;
use zksync_prover_keystore::keystore::Keystore;
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion, L1BatchNumber,
};

use crate::{
    artifacts::ArtifactsManager,
    metrics::WITNESS_GENERATOR_METRICS,
    rounds::{basic_circuits::utils::generate_witness, JobManager},
};

mod artifacts;
pub mod job_processor;
mod utils;

#[derive(Clone)]
pub struct BasicCircuitArtifacts {
    pub(super) circuit_urls: Vec<(u8, String)>,
    pub(super) queue_urls: Vec<(u8, String, usize)>,
    pub(super) scheduler_witness: SchedulerCircuitInstanceWitness<
        GoldilocksField,
        CircuitGoldilocksPoseidon2Sponge,
        GoldilocksExt2,
    >,
    pub(super) aux_output_witness: BlockAuxilaryOutputWitness<GoldilocksField>,
}

#[derive(Clone)]
pub struct BasicWitnessGeneratorJob {
    pub(super) block_number: L1BatchNumber,
    pub(super) data: WitnessInputData,
}

#[derive(Debug)]
pub struct BasicWitnessGenerator {
    config: Arc<FriWitnessGeneratorConfig>,
    object_store: Arc<dyn ObjectStore>,
    public_blob_store: Option<Arc<dyn ObjectStore>>,
    prover_connection_pool: ConnectionPool<Prover>,
    protocol_version: ProtocolSemanticVersion,
}

type Witness = (
    Vec<(u8, String)>,
    Vec<(u8, String, usize)>,
    SchedulerCircuitInstanceWitness<
        GoldilocksField,
        CircuitGoldilocksPoseidon2Sponge,
        GoldilocksExt2,
    >,
    BlockAuxilaryOutputWitness<GoldilocksField>,
);

impl BasicWitnessGenerator {
    pub fn new(
        config: FriWitnessGeneratorConfig,
        object_store: Arc<dyn ObjectStore>,
        public_blob_store: Option<Arc<dyn ObjectStore>>,
        prover_connection_pool: ConnectionPool<Prover>,
        protocol_version: ProtocolSemanticVersion,
    ) -> Self {
        Self {
            config: Arc::new(config),
            object_store,
            public_blob_store,
            prover_connection_pool,
            protocol_version,
        }
    }
}

#[async_trait]
impl JobManager for BasicWitnessGenerator {
    type Job = BasicWitnessGeneratorJob;
    type Metadata = L1BatchNumber;

    const ROUND: AggregationRound = AggregationRound::BasicCircuits;
    const SERVICE_NAME: &'static str = "fri_basic_circuit_witness_generator";

    async fn process_job(
        job: BasicWitnessGeneratorJob,
        object_store: Arc<dyn ObjectStore>,
        max_circuits_in_flight: usize,
        started_at: Instant,
    ) -> anyhow::Result<BasicCircuitArtifacts> {
        let BasicWitnessGeneratorJob {
            block_number,
            data: job,
        } = job;

        tracing::info!(
            "Starting witness generation of type {:?} for block {}",
            AggregationRound::BasicCircuits,
            block_number.0
        );

        let (circuit_urls, queue_urls, scheduler_witness, aux_output_witness) =
            generate_witness(block_number, object_store, job, max_circuits_in_flight).await;
        WITNESS_GENERATOR_METRICS.witness_generation_time[&AggregationRound::BasicCircuits.into()]
            .observe(started_at.elapsed());
        tracing::info!(
            "Witness generation for block {} is complete in {:?}",
            block_number.0,
            started_at.elapsed()
        );

        Ok(BasicCircuitArtifacts {
            circuit_urls,
            queue_urls,
            scheduler_witness,
            aux_output_witness,
        })
    }

    async fn prepare_job(
        metadata: L1BatchNumber,
        object_store: &dyn ObjectStore,
        _keystore: Keystore,
    ) -> anyhow::Result<Self::Job> {
        tracing::info!("Processing FRI basic witness-gen for block {}", metadata.0);
        let started_at = Instant::now();
        let job = Self::get_artifacts(&metadata, object_store).await?;

        WITNESS_GENERATOR_METRICS.blob_fetch_time[&AggregationRound::BasicCircuits.into()]
            .observe(started_at.elapsed());

        Ok(job)
    }

    async fn get_metadata(
        _connection_pool: ConnectionPool<Prover>,
        _protocol_version: ProtocolSemanticVersion,
    ) -> anyhow::Result<Option<(u32, Self::Metadata)>> {
        todo!()
    }
}

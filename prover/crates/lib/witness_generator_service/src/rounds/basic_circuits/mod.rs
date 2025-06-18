use std::{sync::Arc, time::Instant};

use async_trait::async_trait;
use circuit_definitions::zkevm_circuits::scheduler::{
    block_header::BlockAuxilaryOutputWitness, input::SchedulerCircuitInstanceWitness,
};
use zkevm_test_harness::boojum::{
    field::goldilocks::{GoldilocksExt2, GoldilocksField},
    gadgets::recursion::recursive_tree_hasher::CircuitGoldilocksPoseidon2Sponge,
};
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::get_current_pod_name;
use zksync_prover_interface::inputs::WitnessInputData;
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion, L1BatchId,
};

use super::JobMetadata;
use crate::{
    artifact_manager::{ArtifactsManager, JobId},
    metrics::WITNESS_GENERATOR_METRICS,
    rounds::{basic_circuits::utils::generate_witness, JobManager, VerificationKeyManager},
};

mod artifacts;
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
    pub(super) batch_id: L1BatchId,
    pub(super) data: WitnessInputData,
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

pub struct BasicCircuits;

#[async_trait]
impl JobManager for BasicCircuits {
    type Job = BasicWitnessGeneratorJob;
    type Metadata = (L1BatchId, Instant);

    const ROUND: AggregationRound = AggregationRound::BasicCircuits;
    const SERVICE_NAME: &'static str = "fri_basic_circuit_witness_generator";

    async fn process_job(
        job: BasicWitnessGeneratorJob,
        object_store: Arc<dyn ObjectStore>,
        max_circuits_in_flight: usize,
    ) -> anyhow::Result<BasicCircuitArtifacts> {
        let BasicWitnessGeneratorJob {
            batch_id,
            data: job,
        } = job;

        let (circuit_urls, queue_urls, scheduler_witness, aux_output_witness) =
            generate_witness(batch_id, object_store, job, max_circuits_in_flight).await;

        Ok(BasicCircuitArtifacts {
            circuit_urls,
            queue_urls,
            scheduler_witness,
            aux_output_witness,
        })
    }

    async fn prepare_job(
        metadata: Self::Metadata,
        object_store: &dyn ObjectStore,
        _keystore: Arc<dyn VerificationKeyManager>,
    ) -> anyhow::Result<Self::Job> {
        let (metadata, started_at) = metadata;
        let job = Self::get_artifacts(&metadata, object_store).await?;

        WITNESS_GENERATOR_METRICS.blob_fetch_time[&AggregationRound::BasicCircuits.into()]
            .observe(started_at.elapsed());

        Ok(job)
    }

    async fn get_metadata(
        connection_pool: ConnectionPool<Prover>,
        protocol_version: ProtocolSemanticVersion,
    ) -> anyhow::Result<Option<Self::Metadata>> {
        let pod_name = get_current_pod_name();
        let Some(batch_id) = connection_pool
            .connection()
            .await
            .unwrap()
            .fri_basic_witness_generator_dal()
            .get_next_basic_circuit_witness_job(protocol_version, &pod_name)
            .await
        else {
            return Ok(None)
        };
        let started_at = Instant::now();
        Ok(Some((batch_id, started_at)))
    }
}

impl JobMetadata for (L1BatchId, Instant) {
    fn job_id(&self) -> JobId {
        JobId::new(self.0.batch_number().0, self.0.chain_id())
    }
    fn started_at(&self) -> Instant {
        self.1
    }
}

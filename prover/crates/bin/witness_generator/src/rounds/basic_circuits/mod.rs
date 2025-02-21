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
use zksync_prover_keystore::keystore::Keystore;
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion, L1BatchNumber,
    L2ChainId,
};

use crate::{
    artifacts::ArtifactsManager,
    metrics::WITNESS_GENERATOR_METRICS,
    rounds::{basic_circuits::utils::generate_witness, JobManager},
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
    pub(super) chain_id: L2ChainId,
    pub(super) block_number: L1BatchNumber,
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
    type Metadata = (L2ChainId, L1BatchNumber);

    const ROUND: AggregationRound = AggregationRound::BasicCircuits;
    const SERVICE_NAME: &'static str = "fri_basic_circuit_witness_generator";

    async fn process_job(
        job: BasicWitnessGeneratorJob,
        object_store: Arc<dyn ObjectStore>,
        max_circuits_in_flight: usize,
        started_at: Instant,
    ) -> anyhow::Result<BasicCircuitArtifacts> {
        let BasicWitnessGeneratorJob {
            chain_id,
            block_number,
            data: job,
        } = job;

        tracing::info!(
            "Starting witness generation of type {:?} for chain {} block {}",
            AggregationRound::BasicCircuits,
            chain_id.as_u64(),
            block_number.0
        );

        let (circuit_urls, queue_urls, scheduler_witness, aux_output_witness) = generate_witness(
            block_number,
            chain_id,
            object_store,
            job,
            max_circuits_in_flight,
        )
        .await;
        WITNESS_GENERATOR_METRICS.witness_generation_time[&AggregationRound::BasicCircuits.into()]
            .observe(started_at.elapsed());
        tracing::info!(
            "Witness generation for chain {} block {} is complete in {:?}",
            chain_id.as_u64(),
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
        metadata: (L2ChainId, L1BatchNumber),
        object_store: &dyn ObjectStore,
        _keystore: Keystore,
    ) -> anyhow::Result<Self::Job> {
        tracing::info!(
            "Processing FRI basic witness-gen for chain {} block {}",
            metadata.0.as_u64(),
            metadata.1
        );
        let started_at = Instant::now();
        let job = Self::get_artifacts(&metadata, object_store).await?;

        WITNESS_GENERATOR_METRICS.blob_fetch_time[&AggregationRound::BasicCircuits.into()]
            .observe(started_at.elapsed());

        Ok(job)
    }

    async fn get_metadata(
        connection_pool: ConnectionPool<Prover>,
        protocol_version: ProtocolSemanticVersion,
    ) -> anyhow::Result<Option<(L2ChainId, u32, Self::Metadata)>> {
        let pod_name = get_current_pod_name();
        if let Some((chain_id, l1_batch_number)) = connection_pool
            .connection()
            .await
            .unwrap()
            .fri_basic_witness_generator_dal()
            .get_next_basic_circuit_witness_job(protocol_version, &pod_name)
            .await
        {
            Ok(Some((
                chain_id,
                l1_batch_number.0,
                (chain_id, l1_batch_number),
            )))
        } else {
            Ok(None)
        }
    }
}

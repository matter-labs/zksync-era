use std::time::Instant;

use async_trait::async_trait;
use circuit_definitions::zkevm_circuits::scheduler::{
    block_header::BlockAuxilaryOutputWitness, input::SchedulerCircuitInstanceWitness,
};
use zksync_multivm::circuit_sequencer_api_latest::boojum::{
    field::goldilocks::{GoldilocksExt2, GoldilocksField},
    gadgets::recursion::recursive_tree_hasher::CircuitGoldilocksPoseidon2Sponge,
};
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_fri_types::AuxOutputWitnessWrapper;
use zksync_types::L1BatchNumber;

use crate::{
    basic_circuits::{types::BlobUrls, BasicWitnessGenerator, BasicWitnessGeneratorJob},
    traits::{ArtifactsManager, BlobUrls},
    utils::SchedulerPartialInputWrapper,
};

#[derive(Debug)]
pub struct SchedulerArtifacts<'a> {
    pub(super) block_number: L1BatchNumber,
    pub(super) scheduler_partial_input: SchedulerCircuitInstanceWitness<
        GoldilocksField,
        CircuitGoldilocksPoseidon2Sponge,
        GoldilocksExt2,
    >,
    pub(super) aux_output_witness: BlockAuxilaryOutputWitness<GoldilocksField>,
    pub(super) public_object_store: Option<&'a dyn ObjectStore>,
    pub(super) shall_save_to_public_bucket: bool,
}

#[async_trait]
impl ArtifactsManager for BasicWitnessGenerator {
    type Metadata = L1BatchNumber;
    type InputArtifacts = BasicWitnessGeneratorJob;
    type OutputArtifacts = SchedulerArtifacts<'_>;

    async fn get_artifacts(
        metadata: &Self::Metadata,
        object_store: &dyn ObjectStore,
    ) -> Self::InputArtifacts {
        let l1_batch_number = metadata;
        let job = object_store.get(l1_batch_number).await.unwrap();
        BasicWitnessGeneratorJob { block_number, job }
    }

    async fn save_artifacts(
        artifacts: Self::OutputArtifacts,
        object_store: &dyn ObjectStore,
    ) -> BlobUrls {
        let aux_output_witness_wrapper = AuxOutputWitnessWrapper(artifacts.aux_output_witness);
        if artifacts.shall_save_to_public_bucket {
            artifacts.public_object_store
                .expect("public_object_store shall not be empty while running with shall_save_to_public_bucket config")
                .put(artifacts.block_number, &aux_output_witness_wrapper)
                .await
                .unwrap();
        }
        object_store
            .put(artifacts.block_number, &aux_output_witness_wrapper)
            .await
            .unwrap();
        let wrapper = SchedulerPartialInputWrapper(artifacts.scheduler_partial_input);
        let url = object_store
            .put(artifacts.block_number, &wrapper)
            .await
            .unwrap();

        BlobUrls::Url(url)
    }

    async fn update_database(
        connection_pool: ConnectionPool<Prover>,
        block_number: L1BatchNumber,
        started_at: Instant,
    ) {
        todo!()
    }
}

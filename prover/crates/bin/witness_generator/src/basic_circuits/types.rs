use circuit_definitions::zkevm_circuits::scheduler::{
    block_header::BlockAuxilaryOutputWitness, input::SchedulerCircuitInstanceWitness,
};
use zksync_multivm::circuit_sequencer_api_latest::boojum::{
    field::goldilocks::{GoldilocksExt2, GoldilocksField},
    gadgets::recursion::recursive_tree_hasher::CircuitGoldilocksPoseidon2Sponge,
};
use zksync_prover_interface::inputs::WitnessInputData;
use zksync_types::L1BatchNumber;

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
    pub(super) job: WitnessInputData,
}

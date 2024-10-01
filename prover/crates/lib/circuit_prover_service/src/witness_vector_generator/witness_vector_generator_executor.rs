use zksync_prover_fri_types::circuit_definitions::boojum::{
    cs::implementations::witness::WitnessVec, field::goldilocks::GoldilocksField,
};
use zksync_prover_job_processor::Executor;

use crate::types::witness_vector_generator_payload::WitnessVectorGeneratorPayload;

pub struct WitnessVectorGeneratorExecutor;

impl Executor for WitnessVectorGeneratorExecutor {
    type Input = WitnessVectorGeneratorPayload;
    type Output = WitnessVec<GoldilocksField>;

    fn execute(input: Self::Input) -> anyhow::Result<Self::Output> {
        let WitnessVectorGeneratorPayload {
            circuit,
            finalization_hints,
        } = input;
        circuit.synthesize_vector(finalization_hints)
    }
}

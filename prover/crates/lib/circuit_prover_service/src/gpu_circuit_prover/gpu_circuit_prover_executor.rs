use anyhow::Context;
use zksync_prover_fri_types::FriProofWrapper;
use zksync_prover_job_processor::Executor;

use crate::types::circuit_prover_payload::GpuCircuitProverPayload;

pub struct GpuCircuitProverExecutor;

impl Executor for GpuCircuitProverExecutor {
    type Input = GpuCircuitProverPayload;
    type Output = FriProofWrapper;

    fn execute(input: Self::Input) -> anyhow::Result<Self::Output> {
        let GpuCircuitProverPayload {
            circuit,
            witness_vector,
            setup_data,
        } = input;

        let proof_wrapper = circuit
            .prove(witness_vector, setup_data)
            .context("failed to gpu prove circuit")?;
        Ok(proof_wrapper)
    }
}

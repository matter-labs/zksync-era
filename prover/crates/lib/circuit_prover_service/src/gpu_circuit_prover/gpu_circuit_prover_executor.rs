use anyhow::Context;
use shivini::ProverContext;
use zksync_prover_fri_types::FriProofWrapper;
use zksync_prover_job_processor::Executor;
use zksync_types::prover_dal::FriProverJobMetadata;

use crate::types::circuit_prover_payload::GpuCircuitProverPayload;

pub struct GpuCircuitProverExecutor {
    prover_context: ProverContext,
}

impl GpuCircuitProverExecutor {
    pub fn new(prover_context: ProverContext) -> Self {
        Self { prover_context }
    }
}

impl Executor for GpuCircuitProverExecutor {
    type Input = GpuCircuitProverPayload;
    type Output = FriProofWrapper;
    type Metadata = FriProverJobMetadata;

    fn execute(&self, input: Self::Input) -> anyhow::Result<Self::Output> {
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

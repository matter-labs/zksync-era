use std::time::Instant;

use anyhow::Context;
use shivini::ProverContext;
use zksync_prover_fri_types::FriProofWrapper;
use zksync_prover_job_processor::Executor;
use zksync_types::prover_dal::FriProverJobMetadata;

use crate::{
    metrics::CIRCUIT_PROVER_METRICS, types::circuit_prover_payload::GpuCircuitProverPayload,
};

/// GpuCircuitProver executor implementation.
/// Generates circuit proof & verifies it.
/// NOTE: It requires prover context, which is the way Shivini allocates VRAM.
pub struct GpuCircuitProverExecutor {
    _prover_context: ProverContext,
}

impl GpuCircuitProverExecutor {
    pub fn new(prover_context: ProverContext) -> Self {
        Self {
            _prover_context: prover_context,
        }
    }
}

impl Executor for GpuCircuitProverExecutor {
    type Input = GpuCircuitProverPayload;
    type Output = FriProofWrapper;
    type Metadata = FriProverJobMetadata;

    #[tracing::instrument(
        name = "gpu_circuit_prover_executor",
        skip_all,
        fields(l1_batch = % metadata.batch_id)
    )]
    fn execute(
        &self,
        input: Self::Input,
        metadata: Self::Metadata,
    ) -> anyhow::Result<Self::Output> {
        let start_time = Instant::now();
        tracing::info!(
            "Started executing gpu circuit prover job {}, on batch {}, for circuit {}, at round {}",
            metadata.id,
            metadata.batch_id,
            metadata.circuit_id,
            metadata.aggregation_round
        );
        let GpuCircuitProverPayload {
            circuit_wrapper,
            witness_vector,
            setup_data,
        } = input;

        let proof_wrapper = circuit_wrapper
            .prove(witness_vector, setup_data)
            .context("failed to gpu prove circuit")?;
        tracing::info!(
            "Finished executing gpu circuit prover job {}, on batch {}, for circuit {}, at round {} after {:?}",
            metadata.id,
            metadata.batch_id,
            metadata.circuit_id,
            metadata.aggregation_round,
            start_time.elapsed()
        );
        CIRCUIT_PROVER_METRICS
            .prove_and_verify_time
            .observe(start_time.elapsed());
        Ok(proof_wrapper)
    }
}

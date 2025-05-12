use std::time::Instant;

use anyhow::Context;
use zksync_prover_job_processor::Executor;
use zksync_types::prover_dal::FriProverJobMetadata;

use crate::{
    metrics::WITNESS_VECTOR_GENERATOR_METRICS,
    types::{
        witness_vector_generator_execution_output::WitnessVectorGeneratorExecutionOutput,
        witness_vector_generator_payload::WitnessVectorGeneratorPayload,
    },
};

/// WitnessVectorGenerator executor implementation.
/// Synthesizes witness vectors to be later be used in GPU circuit proving.
#[derive(Debug)]
pub struct WitnessVectorGeneratorExecutor;

impl Executor for WitnessVectorGeneratorExecutor {
    type Input = WitnessVectorGeneratorPayload;
    type Output = WitnessVectorGeneratorExecutionOutput;
    type Metadata = FriProverJobMetadata;

    #[tracing::instrument(
        name = "witness_vector_generator_executor",
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
            "Started executing witness vector generator job {}, on batch {}, for circuit {}, at round {}",
            metadata.id,
            metadata.batch_id,
            metadata.circuit_id,
            metadata.aggregation_round
        );
        let WitnessVectorGeneratorPayload {
            circuit_wrapper,
            finalization_hints,
        } = input;
        let witness_vector = circuit_wrapper
            .synthesize_vector(finalization_hints)
            .context("failed to generate witness vector")?;
        tracing::info!(
            "Finished executing witness vector generator job {}, on batch {}, for circuit {}, at round {} in {:?}",
            metadata.id,
            metadata.batch_id,
            metadata.circuit_id,
            metadata.aggregation_round,
            start_time.elapsed()
        );
        WITNESS_VECTOR_GENERATOR_METRICS
            .synthesize_time
            .observe(start_time.elapsed());
        Ok(WitnessVectorGeneratorExecutionOutput {
            circuit_wrapper,
            witness_vector,
        })
    }
}

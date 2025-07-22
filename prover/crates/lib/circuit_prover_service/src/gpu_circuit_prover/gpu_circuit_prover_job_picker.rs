use std::{collections::HashMap, sync::Arc, time::Instant};

use anyhow::Context;
use async_trait::async_trait;
use zksync_prover_fri_types::ProverServiceDataKey;
use zksync_prover_job_processor::JobPicker;
use zksync_types::prover_dal::FriProverJobMetadata;

use crate::{
    gpu_circuit_prover::GpuCircuitProverExecutor,
    metrics::CIRCUIT_PROVER_METRICS,
    types::{
        circuit_prover_payload::GpuCircuitProverPayload, setup_data::GoldilocksGpuProverSetupData,
        witness_vector_generator_execution_output::WitnessVectorGeneratorExecutionOutput,
    },
};

/// GpuCircuitProver job picker implementation.
/// Retrieves job & data from WVG job saver.
#[derive(Debug)]
pub struct GpuCircuitProverJobPicker {
    receiver:
        tokio::sync::mpsc::Receiver<(WitnessVectorGeneratorExecutionOutput, FriProverJobMetadata)>,
    setup_data_cache: HashMap<ProverServiceDataKey, Arc<GoldilocksGpuProverSetupData>>,
}

impl GpuCircuitProverJobPicker {
    pub fn new(
        receiver: tokio::sync::mpsc::Receiver<(
            WitnessVectorGeneratorExecutionOutput,
            FriProverJobMetadata,
        )>,
        setup_data_cache: HashMap<ProverServiceDataKey, Arc<GoldilocksGpuProverSetupData>>,
    ) -> Self {
        Self {
            receiver,
            setup_data_cache,
        }
    }
}

#[async_trait]
impl JobPicker for GpuCircuitProverJobPicker {
    type ExecutorType = GpuCircuitProverExecutor;

    async fn pick_job(
        &mut self,
    ) -> anyhow::Result<Option<(GpuCircuitProverPayload, FriProverJobMetadata)>> {
        let start_time = Instant::now();
        tracing::info!("Started picking gpu circuit prover job");

        let (wvg_output, metadata) = self
            .receiver
            .recv()
            .await
            .context("no witness vector generators are available, stopping...")?;
        let WitnessVectorGeneratorExecutionOutput {
            circuit_wrapper,
            witness_vector,
        } = wvg_output;

        let key = ProverServiceDataKey {
            circuit_id: metadata.circuit_id,
            stage: metadata.aggregation_round.into(),
        }
        .crypto_setup_key();
        let setup_data = self
            .setup_data_cache
            .get(&key)
            .context("failed to retrieve setup data from cache")?
            .clone();

        let payload = GpuCircuitProverPayload {
            circuit_wrapper,
            witness_vector,
            setup_data,
        };
        tracing::info!(
            "Finished picking gpu circuit prover job {}, on batch {}, for circuit {}, at round {} in {:?}",
            metadata.id,
            metadata.batch_id,
            metadata.circuit_id,
            metadata.aggregation_round,
            start_time.elapsed()
        );
        CIRCUIT_PROVER_METRICS
            .load_time
            .observe(start_time.elapsed());
        Ok(Some((payload, metadata)))
    }
}

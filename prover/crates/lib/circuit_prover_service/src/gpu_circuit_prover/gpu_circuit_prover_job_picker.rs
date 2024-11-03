use std::{collections::HashMap, sync::Arc};

use anyhow::Context;
use async_trait::async_trait;
use zksync_types::prover_dal::FriProverJobMetadata;

use zksync_prover_fri_types::ProverServiceDataKey;
use zksync_prover_job_processor::JobPicker;
use zksync_prover_keystore::GoldilocksGpuProverSetupData;

use crate::{
    gpu_circuit_prover::GpuCircuitProverExecutor,
    types::{
        circuit_prover_payload::GpuCircuitProverPayload,
        witness_vector_generator_execution_output::WitnessVectorGeneratorExecutionOutput,
    },
};

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
        tracing::info!("Started picking gpu circuit prover job");
        let (wvg_output, metadata) = self
            .receiver
            .recv()
            .await
            .context("no Witness Vector Generators are available")?;
        let WitnessVectorGeneratorExecutionOutput {
            circuit,
            witness_vector,
        } = wvg_output;

        let key = ProverServiceDataKey {
            circuit_id: metadata.circuit_id,
            round: metadata.aggregation_round,
        }
            .crypto_setup_key();
        let setup_data = self
            .setup_data_cache
            .get(&key)
            .context("failed to retrieve setup data from cache")?
            .clone();

        let payload = GpuCircuitProverPayload::new(circuit, witness_vector, setup_data);
        tracing::info!("Finished picking gpu circuit prover job");
        Ok(Some((payload, metadata)))
    }
}

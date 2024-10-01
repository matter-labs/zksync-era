use std::sync::mpsc::Receiver;

use zksync_prover_job_processor::JobPicker;

use crate::{
    gpu_circuit_prover::GpuCircuitProverExecutor,
    types::circuit_prover_payload::GpuCircuitProverPayload,
};

pub struct GpuCircuitProverJobPicker {
    receiver: Receiver<()>,
}

impl GpuCircuitProverJobPicker {
    pub fn new() -> Self {
        Self {}
    }
}

impl JobPicker for GpuCircuitProverJobPicker {
    type ExecutorType = GpuCircuitProverExecutor;
    type Metadata = ();

    async fn pick_job(&self) -> anyhow::Result<Option<(GpuCircuitProverPayload, ())>> {
        let artifact = self
            .receiver
            .recv()
            .await
            .context("no Witness Vector Generators are available")?;
    }
}

use std::{collections::HashMap, sync::Arc};

use shivini::ProverContext;
use tokio_util::sync::CancellationToken;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_fri_types::{
    circuit_definitions::boojum::cs::implementations::setup::FinalizationHintsForProver,
    get_current_pod_name, ProverServiceDataKey,
};
use zksync_prover_job_processor::{Backoff, BackoffAndCancellable, JobRunner};
use zksync_prover_keystore::GoldilocksGpuProverSetupData;
use zksync_types::{protocol_version::ProtocolSemanticVersion, prover_dal::FriProverJobMetadata};

use crate::{
    gpu_circuit_prover::{
        GpuCircuitProverExecutor, GpuCircuitProverJobPicker, GpuCircuitProverJobSaver,
    },
    types::witness_vector_generator_execution_output::WitnessVectorGeneratorExecutionOutput,
    witness_vector_generator::{
        HeavyWitnessVectorMetadataLoader, LightWitnessVectorMetadataLoader,
        SimpleWitnessVectorMetadataLoader, WitnessVectorGeneratorExecutor,
        WitnessVectorGeneratorJobPicker, WitnessVectorGeneratorJobSaver,
        WitnessVectorMetadataLoader,
    },
};

/// Convenience struct helping with building Witness Vector Generator runners.
#[derive(Debug)]
pub struct WvgRunnerBuilder {
    connection_pool: ConnectionPool<Prover>,
    object_store: Arc<dyn ObjectStore>,
    protocol_version: ProtocolSemanticVersion,
    finalization_hints_cache: HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>,
    sender:
        tokio::sync::mpsc::Sender<(WitnessVectorGeneratorExecutionOutput, FriProverJobMetadata)>,
    cancellation_token: CancellationToken,
    pod_name: String,
}

impl WvgRunnerBuilder {
    pub fn new(
        connection_pool: ConnectionPool<Prover>,
        object_store: Arc<dyn ObjectStore>,
        protocol_version: ProtocolSemanticVersion,
        finalization_hints_cache: HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>,
        sender: tokio::sync::mpsc::Sender<(
            WitnessVectorGeneratorExecutionOutput,
            FriProverJobMetadata,
        )>,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            connection_pool,
            object_store,
            protocol_version,
            finalization_hints_cache,
            sender,
            cancellation_token,
            pod_name: get_current_pod_name(),
        }
    }

    /// Witness Vector Generator runner implementation for light jobs.
    pub fn light_wvg_runner(
        &self,
        count: usize,
    ) -> JobRunner<
        WitnessVectorGeneratorExecutor,
        WitnessVectorGeneratorJobPicker<LightWitnessVectorMetadataLoader>,
        WitnessVectorGeneratorJobSaver,
    > {
        let metadata_loader =
            LightWitnessVectorMetadataLoader::new(self.pod_name.clone(), self.protocol_version);

        self.wvg_runner(count, metadata_loader)
    }

    /// Witness Vector Generator runner implementation that prioritizes heavy jobs over light jobs.
    pub fn heavy_wvg_runner(
        &self,
        count: usize,
    ) -> JobRunner<
        WitnessVectorGeneratorExecutor,
        WitnessVectorGeneratorJobPicker<HeavyWitnessVectorMetadataLoader>,
        WitnessVectorGeneratorJobSaver,
    > {
        let metadata_loader =
            HeavyWitnessVectorMetadataLoader::new(self.pod_name.clone(), self.protocol_version);

        self.wvg_runner(count, metadata_loader)
    }

    /// Witness Vector Generator runner implementation that will execute any type of job.
    pub fn simple_wvg_runner(
        &self,
        count: usize,
    ) -> JobRunner<
        WitnessVectorGeneratorExecutor,
        WitnessVectorGeneratorJobPicker<SimpleWitnessVectorMetadataLoader>,
        WitnessVectorGeneratorJobSaver,
    > {
        let metadata_loader =
            SimpleWitnessVectorMetadataLoader::new(self.pod_name.clone(), self.protocol_version);

        self.wvg_runner(count, metadata_loader)
    }

    /// Creates a Witness Vector Generator job runner with specified MetadataLoader.
    /// The MetadataLoader makes the difference between heavy & light WVG runner.
    fn wvg_runner<ML: WitnessVectorMetadataLoader>(
        &self,
        count: usize,
        metadata_loader: ML,
    ) -> JobRunner<
        WitnessVectorGeneratorExecutor,
        WitnessVectorGeneratorJobPicker<ML>,
        WitnessVectorGeneratorJobSaver,
    > {
        let executor = WitnessVectorGeneratorExecutor;
        let job_picker = WitnessVectorGeneratorJobPicker::new(
            self.connection_pool.clone(),
            self.object_store.clone(),
            self.finalization_hints_cache.clone(),
            metadata_loader,
        );
        let job_saver =
            WitnessVectorGeneratorJobSaver::new(self.connection_pool.clone(), self.sender.clone());
        let backoff = Backoff::default();

        JobRunner::new(
            executor,
            job_picker,
            job_saver,
            count,
            Some(BackoffAndCancellable::new(
                backoff,
                self.cancellation_token.clone(),
            )),
        )
    }
}

/// Circuit Prover runner implementation.
pub fn circuit_prover_runner(
    connection_pool: ConnectionPool<Prover>,
    object_store: Arc<dyn ObjectStore>,
    protocol_version: ProtocolSemanticVersion,
    setup_data_cache: HashMap<ProverServiceDataKey, Arc<GoldilocksGpuProverSetupData>>,
    receiver: tokio::sync::mpsc::Receiver<(
        WitnessVectorGeneratorExecutionOutput,
        FriProverJobMetadata,
    )>,
    prover_context: ProverContext,
) -> JobRunner<GpuCircuitProverExecutor, GpuCircuitProverJobPicker, GpuCircuitProverJobSaver> {
    let executor = GpuCircuitProverExecutor::new(prover_context);
    let job_picker = GpuCircuitProverJobPicker::new(receiver, setup_data_cache);
    let job_saver = GpuCircuitProverJobSaver::new(connection_pool, object_store, protocol_version);
    JobRunner::new(executor, job_picker, job_saver, 1, None)
}

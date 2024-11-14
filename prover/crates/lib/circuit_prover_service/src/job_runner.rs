use std::{collections::HashMap, sync::Arc};

use shivini::ProverContext;
use tokio_util::sync::CancellationToken;
use zksync_object_store::ObjectStore;
use zksync_types::{protocol_version::ProtocolSemanticVersion, prover_dal::FriProverJobMetadata};

use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_fri_types::{
    circuit_definitions::boojum::cs::implementations::setup::FinalizationHintsForProver,
    ProverServiceDataKey,
};
use zksync_prover_job_processor::{Backoff, BackoffAndCancellable, JobRunner};
use zksync_prover_keystore::GoldilocksGpuProverSetupData;

use crate::{
    gpu_circuit_prover::{
        GpuCircuitProverExecutor, GpuCircuitProverJobPicker, GpuCircuitProverJobSaver,
    },
    types::witness_vector_generator_execution_output::WitnessVectorGeneratorExecutionOutput,
    witness_vector_generator::{
        WitnessVectorGeneratorExecutor, WitnessVectorGeneratorJobPicker,
        WitnessVectorGeneratorJobSaver,
    },
};
use crate::witness_vector_generator::{HeavyWitnessVectorMetadataLoader, LightWitnessVectorMetadataLoader, WitnessVectorMetadataLoader};

pub fn light_wvg_runner(
    connection_pool: ConnectionPool<Prover>,
    object_store: Arc<dyn ObjectStore>,
    pod_name: String,
    protocol_version: ProtocolSemanticVersion,
    finalization_hints_cache: HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>,
    count: usize,
    sender: tokio::sync::mpsc::Sender<(
        WitnessVectorGeneratorExecutionOutput,
        FriProverJobMetadata,
    )>,
    cancellation_token: CancellationToken,
) -> JobRunner<
    WitnessVectorGeneratorExecutor,
    WitnessVectorGeneratorJobPicker<LightWitnessVectorMetadataLoader>,
    WitnessVectorGeneratorJobSaver,
> {
    let metadata_loader = LightWitnessVectorMetadataLoader::new(pod_name, protocol_version);

    wvg_runner(
        connection_pool,
        object_store,
        finalization_hints_cache,
        count,
        sender,
        metadata_loader,
        cancellation_token,
    )
}

pub fn heavy_wvg_runner(
    connection_pool: ConnectionPool<Prover>,
    object_store: Arc<dyn ObjectStore>,
    pod_name: String,
    protocol_version: ProtocolSemanticVersion,
    finalization_hints_cache: HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>,
    count: usize,
    sender: tokio::sync::mpsc::Sender<(
        WitnessVectorGeneratorExecutionOutput,
        FriProverJobMetadata,
    )>,
    cancellation_token: CancellationToken,
) -> JobRunner<
    WitnessVectorGeneratorExecutor,
    WitnessVectorGeneratorJobPicker<HeavyWitnessVectorMetadataLoader>,
    WitnessVectorGeneratorJobSaver,
> {
    let metadata_loader = HeavyWitnessVectorMetadataLoader::new(pod_name, protocol_version);
    wvg_runner(
        connection_pool,
        object_store,
        finalization_hints_cache,
        count,
        sender,
        metadata_loader,
        cancellation_token,
    )
}

pub fn wvg_runner<ML: WitnessVectorMetadataLoader>(
    connection_pool: ConnectionPool<Prover>,
    object_store: Arc<dyn ObjectStore>,
    finalization_hints_cache: HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>,
    count: usize,
    sender: tokio::sync::mpsc::Sender<(
        WitnessVectorGeneratorExecutionOutput,
        FriProverJobMetadata,
    )>,
    metadata_loader: ML,
    cancellation_token: CancellationToken,
) -> JobRunner<
    WitnessVectorGeneratorExecutor,
    WitnessVectorGeneratorJobPicker<ML>,
    WitnessVectorGeneratorJobSaver,
> {
    let executor = WitnessVectorGeneratorExecutor;
    let job_picker = WitnessVectorGeneratorJobPicker::new(
        connection_pool.clone(),
        object_store.clone(),
        finalization_hints_cache,
        metadata_loader,
    );
    let job_saver = WitnessVectorGeneratorJobSaver::new(connection_pool, sender);
    let backoff = Backoff::default();

    JobRunner::new(executor, job_picker, job_saver, count, Some(BackoffAndCancellable::new(backoff, cancellation_token)))
}

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

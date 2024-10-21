use std::{collections::HashMap, sync::Arc};

use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_fri_types::{
    circuit_definitions::boojum::cs::implementations::setup::FinalizationHintsForProver,
    ProverServiceDataKey,
};
use zksync_prover_job_processor::JobRunner;
use zksync_types::{protocol_version::ProtocolSemanticVersion, prover_dal::FriProverJobMetadata};

use crate::{
    types::witness_vector_generator_execution_output::WitnessVectorGeneratorExecutionOutput,
    witness_vector_generator::{
        WitnessVectorGeneratorExecutor, WitnessVectorGeneratorJobPicker,
        WitnessVectorGeneratorJobSaver, WvgJobType,
    },
};

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
) -> JobRunner<
    WitnessVectorGeneratorExecutor,
    WitnessVectorGeneratorJobPicker,
    WitnessVectorGeneratorJobSaver,
> {
    wvg_runner(
        connection_pool,
        object_store,
        pod_name,
        protocol_version,
        finalization_hints_cache,
        count,
        sender,
        WvgJobType::Light,
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
) -> JobRunner<
    WitnessVectorGeneratorExecutor,
    WitnessVectorGeneratorJobPicker,
    WitnessVectorGeneratorJobSaver,
> {
    wvg_runner(
        connection_pool,
        object_store,
        pod_name,
        protocol_version,
        finalization_hints_cache,
        count,
        sender,
        WvgJobType::Heavy,
    )
}

pub fn wvg_runner(
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
    wvg_job_type: WvgJobType,
) -> JobRunner<
    WitnessVectorGeneratorExecutor,
    WitnessVectorGeneratorJobPicker,
    WitnessVectorGeneratorJobSaver,
> {
    let executor = WitnessVectorGeneratorExecutor;
    let job_picker = WitnessVectorGeneratorJobPicker::new(
        connection_pool.clone(),
        object_store.clone(),
        pod_name,
        protocol_version,
        finalization_hints_cache,
        wvg_job_type,
    );
    let job_saver = WitnessVectorGeneratorJobSaver::new(connection_pool, sender);
    JobRunner::new(executor, job_picker, job_saver, count)
}

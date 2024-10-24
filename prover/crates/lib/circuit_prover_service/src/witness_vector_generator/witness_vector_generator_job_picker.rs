use std::{collections::HashMap, sync::Arc};

use anyhow::Context;
use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{Connection, ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::{
            cs::implementations::setup::FinalizationHintsForProver,
            gadgets::queue::full_state_queue::FullStateCircuitQueueRawWitness,
        },
        circuit_definitions::base_layer::ZkSyncBaseLayerCircuit,
    },
    keys::RamPermutationQueueWitnessKey,
    CircuitAuxData, CircuitWrapper, ProverServiceDataKey, RamPermutationQueueWitness,
};
use zksync_prover_job_processor::JobPicker;
use zksync_types::{
    protocol_version::ProtocolSemanticVersion, prover_dal::FriProverJobMetadata, L1BatchNumber,
};

use crate::{
    types::{circuit::Circuit, witness_vector_generator_payload::WitnessVectorGeneratorPayload},
    witness_vector_generator::WitnessVectorGeneratorExecutor,
};

#[derive(Debug)]
pub enum WvgJobType {
    Light,
    Heavy,
}

#[derive(Debug)]
pub struct WitnessVectorGeneratorJobPicker {
    connection_pool: ConnectionPool<Prover>,
    object_store: Arc<dyn ObjectStore>,
    pod_name: String,
    protocol_version: ProtocolSemanticVersion,
    finalization_hints_cache: HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>,
    wvg_job_type: WvgJobType,
}

impl WitnessVectorGeneratorJobPicker {
    pub fn new(
        connection_pool: ConnectionPool<Prover>,
        object_store: Arc<dyn ObjectStore>,
        pod_name: String,
        protocol_version: ProtocolSemanticVersion,
        finalization_hints_cache: HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>,
        wvg_job_type: WvgJobType,
    ) -> Self {
        Self {
            connection_pool,
            object_store,
            pod_name,
            protocol_version,
            finalization_hints_cache,
            wvg_job_type,
        }
    }

    async fn fill_witness(
        &self,
        circuit: ZkSyncBaseLayerCircuit,
        aux_data: CircuitAuxData,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Circuit> {
        if let ZkSyncBaseLayerCircuit::RAMPermutation(circuit_instance) = circuit {
            let sorted_witness_key = RamPermutationQueueWitnessKey {
                block_number: l1_batch_number,
                circuit_subsequence_number: aux_data.circuit_subsequence_number as usize,
                is_sorted: true,
            };
            let sorted_witness: RamPermutationQueueWitness = self
                .object_store
                .get(sorted_witness_key)
                .await
                .context("failed to load sorted witness key")?;

            let unsorted_witness_key = RamPermutationQueueWitnessKey {
                block_number: l1_batch_number,
                circuit_subsequence_number: aux_data.circuit_subsequence_number as usize,
                is_sorted: false,
            };
            let unsorted_witness: RamPermutationQueueWitness = self
                .object_store
                .get(unsorted_witness_key)
                .await
                .context("failed to load unsorted witness key")?;

            let mut witness = circuit_instance.witness.take().unwrap();
            witness.unsorted_queue_witness = FullStateCircuitQueueRawWitness {
                elements: unsorted_witness.witness.into(),
            };
            witness.sorted_queue_witness = FullStateCircuitQueueRawWitness {
                elements: sorted_witness.witness.into(),
            };
            circuit_instance.witness.store(Some(witness));

            return Ok(Circuit::Base(ZkSyncBaseLayerCircuit::RAMPermutation(
                circuit_instance,
            )));
        }
        Err(anyhow::anyhow!(
            "unexpected circuit received with partial witness, expected RAM permutation, got {:?}",
            circuit.short_description()
        ))
    }

    async fn get_metadata(
        &self,
        mut connection: Connection<'_, Prover>,
    ) -> Option<FriProverJobMetadata> {
        if let WvgJobType::Heavy = self.wvg_job_type {
            let metadata = connection
                .fri_prover_jobs_dal()
                .get_heavy_job(self.protocol_version, &self.pod_name)
                .await;
            if metadata.is_some() {
                return metadata;
            }
            // if let Some(metadata) = connection.fri_prover_jobs_dal().get_heavy_task(self.protocol_version, &self.pod_name).await {
            //     return Some(metadata);
            // }
        }
        connection
            .fri_prover_jobs_dal()
            .get_light_job(self.protocol_version, &self.pod_name)
            .await
    }
}

#[async_trait]
impl JobPicker for WitnessVectorGeneratorJobPicker {
    type ExecutorType = WitnessVectorGeneratorExecutor;
    async fn pick_job(
        &self,
    ) -> anyhow::Result<Option<(WitnessVectorGeneratorPayload, FriProverJobMetadata)>> {
        let mut connection = self
            .connection_pool
            .connection()
            .await
            .context("failed to get db connection")?;
        let metadata = match self.get_metadata(connection).await {
            None => return Ok(None),
            Some(metadata) => metadata,
        };

        let circuit_wrapper = self
            .object_store
            .get(metadata.into())
            .await
            .context("failed to get circuit_wrapper from object store")?;
        let circuit = match circuit_wrapper {
            CircuitWrapper::Base(circuit) => Circuit::Base(circuit),
            CircuitWrapper::Recursive(circuit) => Circuit::Recursive(circuit),
            CircuitWrapper::BasePartial((circuit, aux_data)) => self
                .fill_witness(circuit, aux_data, metadata.block_number)
                .await
                .context("failed to fill witness")?,
        };

        let key = ProverServiceDataKey {
            circuit_id: metadata.circuit_id,
            round: metadata.aggregation_round,
        }
        .crypto_setup_key();
        let finalization_hints = self
            .finalization_hints_cache
            .get(&key)
            .context("failed to retrieve finalization key from cache")?
            .clone();

        let payload = WitnessVectorGeneratorPayload::new(circuit, finalization_hints);
        Ok(Some((payload, metadata)))
    }
}

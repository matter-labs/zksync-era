use std::{collections::HashMap, sync::Arc, time::Instant};

use anyhow::Context;
use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
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
use zksync_types::{prover_dal::FriProverJobMetadata, L1BatchNumber};

use crate::{
    metrics::WITNESS_VECTOR_GENERATOR_METRICS,
    types::{circuit::Circuit, witness_vector_generator_payload::WitnessVectorGeneratorPayload},
    witness_vector_generator::{
        witness_vector_generator_metadata_loader::WitnessVectorMetadataLoader,
        WitnessVectorGeneratorExecutor,
    },
};

/// WitnessVectorGenerator job picker implementation.
/// Picks job from database (via MetadataLoader) and gets data from object store.
#[derive(Debug)]
pub struct WitnessVectorGeneratorJobPicker<ML: WitnessVectorMetadataLoader> {
    connection_pool: ConnectionPool<Prover>,
    object_store: Arc<dyn ObjectStore>,
    finalization_hints_cache: HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>,
    metadata_loader: ML,
}

impl<ML: WitnessVectorMetadataLoader> WitnessVectorGeneratorJobPicker<ML> {
    pub fn new(
        connection_pool: ConnectionPool<Prover>,
        object_store: Arc<dyn ObjectStore>,
        finalization_hints_cache: HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>,
        metadata_loader: ML,
    ) -> Self {
        Self {
            connection_pool,
            object_store,
            finalization_hints_cache,
            metadata_loader,
        }
    }

    /// Hydrates job data with witness information which is stored separately.
    /// This is done in order to save RAM & storage.
    // TODO: Once new BWG is done, this won't be necessary.
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
}

#[async_trait]
impl<ML: WitnessVectorMetadataLoader> JobPicker for WitnessVectorGeneratorJobPicker<ML> {
    type ExecutorType = WitnessVectorGeneratorExecutor;
    async fn pick_job(
        &mut self,
    ) -> anyhow::Result<Option<(WitnessVectorGeneratorPayload, FriProverJobMetadata)>> {
        let start_time = Instant::now();
        tracing::info!("Started picking witness vector generator job");
        let connection = self
            .connection_pool
            .connection()
            .await
            .context("failed to get db connection")?;
        let metadata = match self.metadata_loader.load_metadata(connection).await {
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
            stage: metadata.aggregation_round.into(),
        }
        .crypto_setup_key();
        let finalization_hints = self
            .finalization_hints_cache
            .get(&key)
            .context("failed to retrieve finalization key from cache")?
            .clone();

        let payload = WitnessVectorGeneratorPayload {
            circuit,
            finalization_hints,
        };
        tracing::info!(
            "Finished picking witness vector generator job {}, on batch {}, for circuit {}, at round {} in {:?}",
            metadata.id,
            metadata.block_number,
            metadata.circuit_id,
            metadata.aggregation_round,
            start_time.elapsed()
        );
        WITNESS_VECTOR_GENERATOR_METRICS
            .pick_time
            .observe(start_time.elapsed());
        Ok(Some((payload, metadata)))
    }
}

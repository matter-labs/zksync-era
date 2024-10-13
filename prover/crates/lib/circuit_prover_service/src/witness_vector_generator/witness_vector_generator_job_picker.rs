use std::{collections::HashMap, sync::Arc};

use anyhow::Context;
use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
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
pub struct WitnessVectorGeneratorJobPicker {
    connection_pool: ConnectionPool<Prover>,
    object_store: Arc<dyn ObjectStore>,
    pod_name: String,
    protocol_version: ProtocolSemanticVersion,
    finalization_hints_cache: HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>,
}

impl WitnessVectorGeneratorJobPicker {
    pub fn new(
        connection_pool: ConnectionPool<Prover>,
        object_store: Arc<dyn ObjectStore>,
        pod_name: String,
        protocol_version: ProtocolSemanticVersion,
        finalization_hints_cache: HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>,
    ) -> Self {
        Self {
            connection_pool,
            object_store,
            pod_name,
            protocol_version,
            finalization_hints_cache,
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
}

#[async_trait]
impl JobPicker for WitnessVectorGeneratorJobPicker {
    type ExecutorType = WitnessVectorGeneratorExecutor;
    type Metadata = FriProverJobMetadata;

    async fn pick_job(
        &self,
    ) -> anyhow::Result<Option<(WitnessVectorGeneratorPayload, FriProverJobMetadata)>> {
        let mut connection = self
            .connection_pool
            .connection()
            .await
            .context("failed to get db connection")?;
        let metadata = match connection
            .fri_prover_jobs_dal()
            .get_job(self.protocol_version, &self.pod_name)
            .await
        {
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
        };
        let finalization_hints = self
            .finalization_hints_cache
            .get(&key)
            .context("failed to retrieve finalization key from cache")?
            .clone();

        let payload = WitnessVectorGeneratorPayload::new(circuit, finalization_hints);
        Ok(Some((payload, metadata)))
    }
}

#[cfg(test)]
mod tests {
    use zksync_object_store::MockObjectStore;

    use super::*;

    #[tokio::test]
    async fn test_success() -> anyhow::Result<()> {
        let connection_pool = ConnectionPool::<Prover>::prover_test_pool().await;
        let object_store = MockObjectStore::arc();
        object_store.put();
        Ok(())
    }
    // use std::sync::Arc;
    // use zksync_object_store::bincode;
    // use zksync_types::basic_fri_types::AggregationRound;
    // use zksync_prover_fri_types::{CircuitWrapper, ProverServiceDataKey};
    // use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::setup::FinalizationHintsForProver;
    // use super::*;
    // use zksync_prover_job_processor::Executor;
    // use zksync_prover_keystore::keystore::Keystore;
    // use crate::types::circuit::Circuit;
    //
    // #[tokio::test]
    // async fn test_success() -> anyhow::Result<()> {
    //     let executor = WitnessVectorGeneratorExecutor;
    //     let serialized = std::fs::read("tests/data/node_circuit.bin")?;
    //     let circuit: CircuitWrapper = bincode::deserialize(&serialized)?;
    //     let circuit = if let CircuitWrapper::Recursive(inner_circuit) = circuit{
    //         inner_circuit
    //     } else {
    //         return Err(anyhow::anyhow!("Wrong circuit type"));
    //     };
    //
    //     let node_finalization_hints = Arc::new(Keystore::locate().load_finalization_hints(ProverServiceDataKey::new(2, AggregationRound::NodeAggregation))?);
    //
    //     let input = WitnessVectorGeneratorPayload::new(
    //         Circuit::Recursive(circuit),
    //         node_finalization_hints,
    //     );
    //     let output = executor.execute(input)?;
    //     let expected_output: WitnessVec<GoldilocksField> = bincode::deserialize(&std::fs::read("tests/data/node_witness_vector.bin")?)?;
    //     // TODO: we should implement PartialEq for WitnessVec, then we can do assert_eq!(output, expected_output);
    //     for (a, b) in output.public_inputs_locations.iter().zip(expected_output.public_inputs_locations.iter()) {
    //         assert_eq!(a, b);
    //     }
    //     for (a, b) in output.all_values.iter().zip(expected_output.all_values.iter()) {
    //         assert_eq!(a, b);
    //     }
    //     for (a, b) in output.multiplicities.iter().zip(expected_output.multiplicities.iter()) {
    //         assert_eq!(a, b);
    //     }
    //     Ok(())
    // }
    //
    // #[tokio::test]
    // #[should_panic(expected = "assertion `left == right` failed")]
    // async fn test_failure() {
    //     let executor = WitnessVectorGeneratorExecutor;
    //     let serialized = std::fs::read("tests/data/node_circuit.bin").expect("failed to read circuit from file");
    //     let circuit: CircuitWrapper = bincode::deserialize(&serialized).expect("failed to deserialize circuit");
    //     let circuit = if let CircuitWrapper::Recursive(inner_circuit) = circuit {
    //         inner_circuit
    //     } else {
    //         panic!("read unexpected circuit type from file");
    //     };
    //
    //     let input = WitnessVectorGeneratorPayload::new(
    //         Circuit::Recursive(circuit),
    //         Arc::new(FinalizationHintsForProver::default()),
    //     );
    //
    //     // this assert won't be checked, as the `.execute()` will panic
    //     assert!(executor.execute(input).is_ok());
    // }
}

use anyhow::Context;
use zksync_prover_fri_types::circuit_definitions::boojum::{
    cs::implementations::witness::WitnessVec, field::goldilocks::GoldilocksField,
};
use zksync_prover_job_processor::Executor;
use zksync_types::prover_dal::FriProverJobMetadata;

use crate::types::{
    witness_vector_generator_execution_output::WitnessVectorGeneratorExecutionOutput,
    witness_vector_generator_payload::WitnessVectorGeneratorPayload,
};

pub struct WitnessVectorGeneratorExecutor;

impl Executor for WitnessVectorGeneratorExecutor {
    type Input = WitnessVectorGeneratorPayload;
    type Output = WitnessVectorGeneratorExecutionOutput;
    type Metadata = FriProverJobMetadata;

    fn execute(&self, input: Self::Input) -> anyhow::Result<Self::Output> {
        let inner_circuit = input.circuit();
        let finalization_hints = input.finalization_hints();
        let vector = inner_circuit
            .synthesize_vector(finalization_hints)
            .context("failed to generate witness vector")?;
        Ok(WitnessVectorGeneratorExecutionOutput::new(
            input.into_circuit(),
            vector,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use zksync_object_store::bincode;
    use zksync_prover_fri_types::{
        circuit_definitions::boojum::cs::implementations::setup::FinalizationHintsForProver,
        CircuitWrapper, ProverServiceDataKey,
    };
    use zksync_prover_job_processor::Executor;
    use zksync_prover_keystore::keystore::Keystore;
    use zksync_types::basic_fri_types::AggregationRound;

    use super::*;
    use crate::types::circuit::Circuit;

    #[tokio::test]
    async fn test_success() -> anyhow::Result<()> {
        let executor = WitnessVectorGeneratorExecutor;
        let serialized = std::fs::read("tests/data/node_circuit.bin")?;
        let circuit: CircuitWrapper = bincode::deserialize(&serialized)?;
        let circuit = if let CircuitWrapper::Recursive(inner_circuit) = circuit {
            inner_circuit
        } else {
            return Err(anyhow::anyhow!("Wrong circuit type"));
        };

        let node_finalization_hints = Arc::new(Keystore::locate().load_finalization_hints(
            ProverServiceDataKey::new(2, AggregationRound::NodeAggregation),
        )?);

        let input = WitnessVectorGeneratorPayload::new(
            Circuit::Recursive(circuit),
            node_finalization_hints,
        );
        let output = executor.execute(input)?;
        let expected_output: WitnessVec<GoldilocksField> =
            bincode::deserialize(&std::fs::read("tests/data/node_witness_vector.bin")?)?;
        // TODO: we should implement PartialEq for WitnessVec, then we can do assert_eq!(output, expected_output);
        for (a, b) in output
            .public_inputs_locations
            .iter()
            .zip(expected_output.public_inputs_locations.iter())
        {
            assert_eq!(a, b);
        }
        for (a, b) in output
            .all_values
            .iter()
            .zip(expected_output.all_values.iter())
        {
            assert_eq!(a, b);
        }
        for (a, b) in output
            .multiplicities
            .iter()
            .zip(expected_output.multiplicities.iter())
        {
            assert_eq!(a, b);
        }
        Ok(())
    }

    #[tokio::test]
    #[should_panic(expected = "assertion `left == right` failed")]
    async fn test_failure() {
        let executor = WitnessVectorGeneratorExecutor;
        let serialized =
            std::fs::read("tests/data/node_circuit.bin").expect("failed to read circuit from file");
        let circuit: CircuitWrapper =
            bincode::deserialize(&serialized).expect("failed to deserialize circuit");
        let circuit = if let CircuitWrapper::Recursive(inner_circuit) = circuit {
            inner_circuit
        } else {
            panic!("read unexpected circuit type from file");
        };

        let input = WitnessVectorGeneratorPayload::new(
            Circuit::Recursive(circuit),
            Arc::new(FinalizationHintsForProver::default()),
        );

        // this assert won't be checked, as the `.execute()` will panic
        assert!(executor.execute(input).is_ok());
    }
}

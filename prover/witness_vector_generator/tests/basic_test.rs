use std::fs;
use zksync_prover_fri_types::{CircuitWrapper, ProverJob, ProverServiceDataKey};
use zksync_types::proofs::AggregationRound;
use zksync_types::L1BatchNumber;
use zksync_witness_vector_generator::generator::WitnessVectorGenerator;

#[test]
fn test_generate_witness_vector() {
    let filename = "./tests/data/base_layer_main_vm.bin";
    let file = fs::read(filename).expect("failed reading circuit");
    let circuit_wrapper = bincode::deserialize::<CircuitWrapper>(&file)
        .expect("circuit wrapper deserialization failed");
    let key = ProverServiceDataKey {
        circuit_id: 1,
        round: AggregationRound::BasicCircuits,
    };
    let job = ProverJob {
        block_number: L1BatchNumber(1),
        job_id: 1,
        circuit_wrapper,
        setup_data_key: key,
    };
    let vector = WitnessVectorGenerator::generate_witness_vector(job).unwrap();
    assert!(!vector.witness_vector.all_values.is_empty());
    assert!(!vector.witness_vector.multiplicities.is_empty());
    assert!(!vector.witness_vector.public_inputs_locations.is_empty());
    let serialized = bincode::serialize(&vector).expect("failed to serialize witness vector");
    assert!(
        serialized.len() < 1_000_000_000,
        "The size of the serialized vector shall be less than 1GB"
    );
}

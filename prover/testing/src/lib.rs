#![feature(generic_const_exprs)]
pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    use circuit_definitions::{
        circuit_definitions::{
            base_layer::Keccak256RoundFunctionInstanceSynthesisFunction, eip4844::EIP4844Circuit,
        },
        eip4844_proof_config, ZkSyncDefaultRoundFunction, EIP4844_CYCLE_LIMIT,
    };
    use zkevm_test_harness::{
        boojum::{cs::implementations::pow::NoPow, field::goldilocks::GoldilocksField},
        prover_utils::{create_eip4844_setup_data, prove_eip4844_circuit, verify_eip4844_proof},
        utils::generate_eip4844_witness,
    };

    use zkevm_test_harness::boojum::worker::Worker;
    use zksync_prover_fri_types::{
        CircuitWrapper, ProverJob, ProverServiceDataKey, WitnessVectorArtifacts,
    };
    use zksync_vk_setup_data_server_fri::keystore::Keystore;
    use zksync_vk_setup_data_server_fri::ProverSetupData;
    use zksync_witness_vector_generator::generator::WitnessVectorGenerator;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
    

    // To generate setup keys (to run from  ../prover directory):
    // cargo run --release  --bin key_generator -- generate-sk eip4844 --path=/ssd_storage/matter-public/zksync-era/prover/vk_setup_data_generator_server_fri/data/ --setup-path /large_ssd/keys_2024_02/
    // And then: 
    // cargo test --release witness_gen_test
    // And this will write results to /tmp/result.json
    #[test]
    fn witness_gen_test() {
        let circuit = blob_to_circuit(vec![3u8; 4096 * 31]);
        let keystore = Keystore::new("/ssd_storage/matter-public/zksync-era/prover/vk_setup_data_generator_server_fri/data/".to_string(), "/large_ssd/keys_2024_02/".to_string());

        let artifacts = WitnessVectorGenerator::generate_witness_vector_with_keystore(ProverJob {
            block_number: zksync_types::L1BatchNumber(2),
            job_id: 33,
            circuit_wrapper: CircuitWrapper::Eip4844(circuit),
            setup_data_key: ProverServiceDataKey::eip4844(),
        }, &keystore)
        .unwrap();

        std::fs::write(
            "/tmp/result.json",
            serde_json::to_string_pretty(&artifacts).unwrap(),
        )
        .unwrap();
        let serialized: Vec<u8> =
            bincode::serialize(&artifacts).expect("Failed to serialize witness vector artifacts");
        std::fs::write(
                "/tmp/result.bin",
                serialized
        ).unwrap();
        


    }

    fn blob_to_circuit(
        blob: Vec<u8>,
    ) -> EIP4844Circuit<GoldilocksField, ZkSyncDefaultRoundFunction> {
        let (blob_arr, linear_hash, versioned_hash, output_hash) =
            generate_eip4844_witness::<GoldilocksField>(blob);

        use circuit_definitions::circuit_definitions::eip4844::EIP4844Circuit;
        use crossbeam::atomic::AtomicCell;
        use std::collections::VecDeque;
        use std::sync::Arc;
        use zkevm_test_harness::zkevm_circuits::eip_4844::input::BlobChunkWitness;
        use zkevm_test_harness::zkevm_circuits::eip_4844::input::EIP4844CircuitInstanceWitness;
        use zkevm_test_harness::zkevm_circuits::eip_4844::input::EIP4844InputOutputWitness;
        use zkevm_test_harness::zkevm_circuits::eip_4844::input::EIP4844OutputDataWitness;
        use zksync_prover_fri_types::ProverServiceDataKey;

        let blob = blob_arr
            .iter()
            .map(|el| BlobChunkWitness { inner: *el })
            .collect::<Vec<BlobChunkWitness<GoldilocksField>>>();
        let witness = EIP4844CircuitInstanceWitness {
            closed_form_input: EIP4844InputOutputWitness {
                start_flag: true,
                completion_flag: true,
                hidden_fsm_input: (),
                hidden_fsm_output: (),
                observable_input: (),
                observable_output: EIP4844OutputDataWitness {
                    linear_hash,
                    output_hash,
                },
            },
            data_chunks: VecDeque::from(blob),
            linear_hash_output: linear_hash,
            versioned_hash,
        };
        EIP4844Circuit {
            witness: AtomicCell::new(Some(witness)),
            config: Arc::new(EIP4844_CYCLE_LIMIT),
            round_function: ZkSyncDefaultRoundFunction::default().into(),
            expected_public_input: None,
        }
    }

    #[test]
    fn other_test() {
        // Get the setup key
        // Get some random data
        // Generate eip4844

        let blob = vec![3u8; 80000];
        let eip4844_proof_config = eip4844_proof_config();

        let circuit = blob_to_circuit(blob);

        let worker = Worker::new_with_num_threads(8);

        let keystore = Keystore::new("/ssd_storage/matter-public/zksync-era/prover/vk_setup_data_generator_server_fri/data/".to_string(), "/large_ssd/keys_2024_02/".to_string());

        let setup_data = keystore
            .load_cpu_setup_data_for_circuit_type(ProverServiceDataKey::eip4844())
            .unwrap();

        let proof = prove_eip4844_circuit::<NoPow>(
            circuit.clone(),
            &worker,
            eip4844_proof_config,
            &setup_data.setup_base,
            &setup_data.setup,
            &setup_data.setup_tree,
            &setup_data.vk,
            &setup_data.vars_hint,
            &setup_data.wits_hint,
            &setup_data.finalization_hint,
        );

        let is_valid = verify_eip4844_proof::<NoPow>(&circuit, &proof, &setup_data.vk);
        assert!(is_valid);
    }
}

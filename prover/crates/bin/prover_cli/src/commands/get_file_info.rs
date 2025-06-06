use std::fs;

use clap::Args as ClapArgs;
use colored::Colorize;
use zksync_circuit_prover_service::types::circuit_wrapper::CircuitWrapper;
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::{
            field::goldilocks::{GoldilocksExt2, GoldilocksField},
            gadgets::recursion::recursive_tree_hasher::CircuitGoldilocksPoseidon2Sponge,
        },
        circuit_definitions::{
            base_layer::ZkSyncBaseLayerCircuit, recursion_layer::ZkSyncRecursiveLayerCircuit,
        },
        zkevm_circuits::scheduler::input::SchedulerCircuitInstanceWitness,
    },
    FriProofWrapper,
};
use zksync_prover_interface::{
    inputs::WitnessInputMerklePaths,
    outputs::{L1BatchProofForL1, TypedL1BatchProofForL1},
};
use zksync_types::{u256_to_h256, H256};

#[derive(ClapArgs)]
pub struct Args {
    #[clap(short, long)]
    file_path: String,
}

fn pretty_print_size_hint(size_hint: (Option<usize>, Option<usize>)) {
    println!(
        "Circuit size: trace length: {:?} something??: {:?}",
        size_hint.0, size_hint.1
    );
}

fn pretty_print_scheduler_witness(
    witness: &SchedulerCircuitInstanceWitness<
        GoldilocksField,
        CircuitGoldilocksPoseidon2Sponge,
        GoldilocksExt2,
    >,
) {
    println!("Scheduler witness info");
    println!("  Previous block data: ");
    println!(
        "    Enumeration counter: {:?}",
        witness.prev_block_data.per_shard_states[0].enumeration_counter[0]
    );
    println!(
        "    State root: 0x{}",
        hex::encode(witness.prev_block_data.per_shard_states[0].state_root)
    );

    println!("  Block meta parameters");
    println!(
        "    bootloader code hash: {:?}",
        witness.block_meta_parameters.bootloader_code_hash
    );
    println!(
        "    aa code hash: {:?}",
        witness.block_meta_parameters.default_aa_code_hash
    );

    println!(
        "  Previous block meta hash: 0x{}",
        hex::encode(witness.previous_block_meta_hash)
    );
    println!(
        "  Previous block aux hash: 0x{}",
        hex::encode(witness.previous_block_aux_hash)
    );

    println!("  EIP 4844 - witnesses: {:?}", witness.eip4844_witnesses);
}

fn pretty_print_circuit_wrapper(circuit: &CircuitWrapper) {
    println!(" == Circuit ==");
    match circuit {
        CircuitWrapper::Base(circuit) => {
            println!(
                "Type: basic. Id: {:?} ({})",
                circuit.numeric_circuit_type(),
                circuit.short_description().bold()
            );
            println!("Geometry: {:?}", circuit.geometry());
            pretty_print_size_hint(circuit.size_hint());

            match circuit {
                ZkSyncBaseLayerCircuit::MainVM(_) => todo!(),
                ZkSyncBaseLayerCircuit::CodeDecommittmentsSorter(_) => todo!(),
                ZkSyncBaseLayerCircuit::CodeDecommitter(_) => todo!(),
                ZkSyncBaseLayerCircuit::LogDemuxer(_) => todo!(),
                ZkSyncBaseLayerCircuit::KeccakRoundFunction(_) => todo!(),
                ZkSyncBaseLayerCircuit::Sha256RoundFunction(_) => todo!(),
                ZkSyncBaseLayerCircuit::ECRecover(circuit) => {
                    println!("Expected public input: {:?}", circuit.expected_public_input);
                    println!("Max ECRecovers per circuit: {:?}", circuit.config);
                }
                ZkSyncBaseLayerCircuit::RAMPermutation(_) => todo!(),
                ZkSyncBaseLayerCircuit::StorageSorter(_) => todo!(),
                ZkSyncBaseLayerCircuit::StorageApplication(circuit) => {
                    let witness = circuit.clone_witness().unwrap();
                    println!(
                        "Initial root hash: {:?}",
                        H256::from_slice(
                            &witness.closed_form_input.observable_input.initial_root_hash
                        )
                    );
                    println!(
                        "Fsm input hash: {:?}",
                        H256::from_slice(
                            &witness.closed_form_input.hidden_fsm_input.current_root_hash
                        )
                    );
                    println!(
                        "Fsm output hash: {:?}",
                        H256::from_slice(
                            &witness
                                .closed_form_input
                                .hidden_fsm_output
                                .current_root_hash
                        )
                    );
                    println!(
                        "Final root hash: {:?}",
                        H256::from_slice(
                            &witness.closed_form_input.observable_output.new_root_hash
                        )
                    );

                    let storage_queue = witness.storage_queue_witness.elements;
                    println!("storage queue elements: {:?}", storage_queue.len());
                    for (i, x) in storage_queue.iter().enumerate() {
                        println!(
                            "{}  element: rw:{:?} {:?} {:?} {:?}",
                            i,
                            x.0.rw_flag,
                            x.0.address,
                            u256_to_h256(x.0.key),
                            u256_to_h256(x.0.written_value)
                        );
                    }
                }
                ZkSyncBaseLayerCircuit::EventsSorter(_) => todo!(),
                ZkSyncBaseLayerCircuit::L1MessagesSorter(_) => todo!(),
                ZkSyncBaseLayerCircuit::L1MessagesHasher(_) => todo!(),
                ZkSyncBaseLayerCircuit::TransientStorageSorter(_) => todo!(),
                ZkSyncBaseLayerCircuit::Secp256r1Verify(_) => todo!(),
                ZkSyncBaseLayerCircuit::EIP4844Repack(_) => todo!(),
                ZkSyncBaseLayerCircuit::Modexp(_) => todo!(),
                ZkSyncBaseLayerCircuit::ECAdd(_) => todo!(),
                ZkSyncBaseLayerCircuit::ECMul(_) => todo!(),
                ZkSyncBaseLayerCircuit::ECPairing(_) => todo!(),
            }
        }
        CircuitWrapper::Recursive(circuit) => {
            println!(
                "Type: basic. Id: {:?} ({})",
                circuit.numeric_circuit_type(),
                circuit.short_description().bold()
            );
            println!("Geometry: {:?}", circuit.geometry());
            pretty_print_size_hint(circuit.size_hint());
            match circuit {
                ZkSyncRecursiveLayerCircuit::SchedulerCircuit(circuit) => {
                    //println!("Expected public input: {:?}", circuit.witness);
                    pretty_print_scheduler_witness(&circuit.witness);
                }
                ZkSyncRecursiveLayerCircuit::NodeLayerCircuit(_) => todo!(),
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForMainVM(_) => todo!(),
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForCodeDecommittmentsSorter(_) => {
                    todo!()
                }
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForCodeDecommitter(_) => todo!(),
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForLogDemuxer(_) => todo!(),
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForKeccakRoundFunction(_) => todo!(),
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForSha256RoundFunction(_) => todo!(),
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForECRecover(_) => todo!(),
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForRAMPermutation(_) => todo!(),
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForStorageSorter(_) => todo!(),
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForStorageApplication(_) => todo!(),
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForEventsSorter(_) => todo!(),
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForL1MessagesSorter(_) => todo!(),
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForL1MessagesHasher(_) => todo!(),
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForTransientStorageSorter(_) => {
                    todo!()
                }
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForSecp256r1Verify(_) => todo!(),
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForEIP4844Repack(_) => todo!(),
                ZkSyncRecursiveLayerCircuit::RecursionTipCircuit(_) => todo!(),
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForModexp(_) => todo!(),
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForECAdd(_) => todo!(),
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForECMul(_) => todo!(),
                ZkSyncRecursiveLayerCircuit::LeafLayerCircuitForECPairing(_) => todo!(),
            }
        }
    }
}

fn pretty_print_proof(result: &FriProofWrapper) {
    println!("{}", "== FRI proof ==".to_string().bold());
    match result {
        FriProofWrapper::Base(proof) => {
            println!(
                "Basic proof {} {}",
                proof.numeric_circuit_type(),
                proof.short_description()
            );
        }
        FriProofWrapper::Recursive(proof) => {
            println!(
                "Recursive proof {} {}",
                proof.numeric_circuit_type(),
                proof.short_description()
            );

            let proof = proof.clone().into_inner();
            println!("Proof config: {:?}", proof.proof_config);
            println!("Proof public inputs: {:?}", proof.public_inputs);
        }
    }
}

fn pretty_print_l1_proof(result: &L1BatchProofForL1) {
    println!("{}", "== Snark wrapped L1 proof ==".to_string().bold());
    println!("AUX info:");

    let aggregation_result_coords = result.aggregation_result_coords();

    println!(
        "  L1 msg linear hash: 0x{}",
        hex::encode(aggregation_result_coords[0])
    );
    println!(
        "  Rollup_state_diff_for_compression: 0x{}",
        hex::encode(aggregation_result_coords[1])
    );
    println!(
        "  bootloader_heap_initial_content: 0x{}",
        hex::encode(aggregation_result_coords[2])
    );
    println!(
        "  events_queue_state: 0x{}",
        hex::encode(aggregation_result_coords[3])
    );

    let inputs = match result.inner() {
        TypedL1BatchProofForL1::Fflonk(proof) => proof.clone().scheduler_proof.inputs,
        TypedL1BatchProofForL1::Plonk(proof) => proof.clone().scheduler_proof.inputs,
    };

    println!("Inputs: {:?}", inputs);
    println!("  This proof will pass on L1, if L1 executor computes the block commitment that is matching exactly the Inputs value above");
}

pub(crate) async fn run(args: Args) -> anyhow::Result<()> {
    let path = args.file_path;
    println!("Reading file {} and guessing the type.", path.bold());

    let bytes = fs::read(path).unwrap();

    // Parsing stuff from `prover_jobs_fri` directory.
    let maybe_circuit: Option<CircuitWrapper> = bincode::deserialize(&bytes).ok();
    if let Some(circuit) = maybe_circuit {
        println!("  Parsing file as CircuitWrapper.");
        pretty_print_circuit_wrapper(&circuit);
        return Ok(());
    }
    println!("  NOT a CircuitWrapper.");
    let maybe_fri_proof: Option<FriProofWrapper> = bincode::deserialize(&bytes).ok();
    if let Some(fri_proof) = maybe_fri_proof {
        println!("  Parsing file as FriProofWrapper.");
        pretty_print_proof(&fri_proof);
        return Ok(());
    }
    println!("  NOT a FriProofWrapper.");

    let maybe_snark_proof: Option<L1BatchProofForL1> = bincode::deserialize(&bytes).ok();
    if let Some(snark_proof) = maybe_snark_proof {
        println!("  Parsing file as L1BatchProofForL1.");
        pretty_print_l1_proof(&snark_proof)
    } else {
        println!("  NOT a L1BatchProof.");
    }

    let maybe_witness_input: Option<WitnessInputMerklePaths> = bincode::deserialize(&bytes).ok();
    if let Some(witness_input) = maybe_witness_input {
        println!("  Parsing file as WitnessInputMerklePaths.");
        println!(
            " Next enumeration index: {}",
            witness_input.next_enumeration_index()
        );
        println!("  merkle paths: {:?}", witness_input.merkle_paths.len());
        let merkle_path = witness_input.merkle_paths[0].clone();
        println!(
            "first root hash: {:?}",
            H256::from_slice(&merkle_path.root_hash)
        );

        let merkle_path = witness_input.merkle_paths.last().unwrap().clone();
        println!(
            "last root hash: {:?}",
            H256::from_slice(&merkle_path.root_hash)
        );
    } else {
        println!("  NOT a WitnessInputMerklePaths.");
    }

    Ok(())
}

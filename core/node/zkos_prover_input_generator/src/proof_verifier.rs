use air_compiler_cli::prover_utils::{
    generate_oracle_data_from_metadata_and_proof_list,
    proof_list_and_metadata_from_program_proof,
};
use execution_utils::ProgramProof;
use zk_os_basic_system::system_implementation::system::BatchPublicInput;
use zksync_l1_contract_interface::zkos_commitment_to_vm_batch_output;
use zksync_types::commitment::{L1BatchWithMetadata, ZkosCommitment};
use zksync_zkos_vm_runner::zkos_conversions::h256_to_bytes32;

pub fn verify_fri_proof(
    previous_batch: L1BatchWithMetadata,
    current_batch: L1BatchWithMetadata,
    input_program_proof: ProgramProof
) {
    let current_batch_commitment = ZkosCommitment::from(&current_batch);
    let current_batch_output= zkos_commitment_to_vm_batch_output(&current_batch_commitment);

    let prev_batch_commitment = ZkosCommitment::from(&previous_batch);

    let pi = BatchPublicInput {
        state_before: h256_to_bytes32(prev_batch_commitment.state_commitment()),
        state_after: h256_to_bytes32(current_batch_commitment.state_commitment()),
        batch_output: current_batch_output.hash().into(),
    };

    let pi_hash_u32s =  pi.hash().chunks_exact(4).map(|chunk| {
        u32::from_be_bytes(chunk.try_into().expect("Slice with incorrect length"))
    }).collect::<Vec<u32>>();

    tracing::info!("Public input hash: {:?}", pi_hash_u32s);

    let (metadata, proof_list) = proof_list_and_metadata_from_program_proof(input_program_proof);

    let oracle_data = generate_oracle_data_from_metadata_and_proof_list(&metadata, &proof_list);
    tracing::info!("Oracle data iterator created with {} items", oracle_data.len());

    let it = oracle_data.into_iter();

    verifier_common::prover::nd_source_std::set_iterator(it);

    // Assume that program proof has only recursion proofs.
    tracing::info!("Running continue recursive");
    assert!(metadata.reduced_proof_count > 0);
    let output = full_statement_verifier::verify_recursion_layer();
    tracing::info!("Output is: {:?}", output);

    assert!(
        verifier_common::prover::nd_source_std::try_read_word().is_none(),
        "Expected that all words from CSR were consumed"
    );
}
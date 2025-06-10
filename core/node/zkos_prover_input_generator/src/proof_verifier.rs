use air_compiler_cli::prover_utils::{
    generate_oracle_data_from_metadata_and_proof_list, proof_list_and_metadata_from_program_proof,
};
use execution_utils::ProgramProof;
use zk_os_basic_system::system_implementation::system::BatchPublicInput;
use zksync_dal::{Connection, Core};
use zksync_l1_contract_interface::i_executor::{
    batch_output_hash_as_register_values, batch_public_input,
};
use zksync_l1_contract_interface::zkos_commitment_to_vm_batch_output;
use zksync_types::commitment::{L1BatchWithMetadata, ZkosCommitment};
use zksync_zkos_vm_runner::zkos_conversions::h256_to_bytes32;

#[not(cfg(feature = "include_verifiers"))]
pub fn verify_fri_proof(
    previous_batch: L1BatchWithMetadata,
    current_batch: L1BatchWithMetadata,
    input_program_proof: ProgramProof,
) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(feature = "include_verifiers")]
pub fn verify_fri_proof(
    previous_batch: L1BatchWithMetadata,
    current_batch: L1BatchWithMetadata,
    input_program_proof: ProgramProof,
) -> anyhow::Result<()> {
    let expected_pi = batch_public_input(&previous_batch, &current_batch);
    let expected_hash_u32s: [u32; 8] = batch_output_hash_as_register_values(&expected_pi);

    let proof_final_register_values: [u32; 16] = extract_final_register_values(input_program_proof);

    tracing::info!("Program final registers: {:?}", proof_final_register_values);
    tracing::info!(
        "Expected values for Public Inputs hash: {:?}",
        expected_hash_u32s
    );

    // compare expected_hash_u32s with the last 8 values of proof_final_register_values
    if (&proof_final_register_values[..8] == &expected_hash_u32s) {
        tracing::info!(
            "Final register values match expected hash values for block {}",
            current_batch.header.number.0
        );
        Ok(())
    } else {
        return Err(anyhow::anyhow!(
            "Final register values do not match expected hash values. Expected: {:?}, got: {:?}",
            expected_hash_u32s,
            &proof_final_register_values[8..]
        ));
    }
}
#[cfg(feature = "include_verifiers")]
fn extract_final_register_values(input_program_proof: ProgramProof) -> [u32; 16] {
    let (metadata, proof_list) = proof_list_and_metadata_from_program_proof(input_program_proof);

    let oracle_data = generate_oracle_data_from_metadata_and_proof_list(&metadata, &proof_list);
    tracing::info!(
        "Oracle data iterator created with {} items",
        oracle_data.len()
    );

    let it = oracle_data.into_iter();

    verifier_common::prover::nd_source_std::set_iterator(it);

    // Assume that program proof has only recursion proofs.
    tracing::info!("Running continue recursive");
    assert!(metadata.reduced_proof_count > 0);

    let final_register_values = full_statement_verifier::verify_recursion_layer();

    assert!(
        verifier_common::prover::nd_source_std::try_read_word().is_none(),
        "Expected that all words from CSR were consumed"
    );
    final_register_values
}

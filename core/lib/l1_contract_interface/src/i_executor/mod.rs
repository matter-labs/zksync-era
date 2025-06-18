//! Different interfaces exposed by the `IExecutor.sol`.

use anyhow::Context;
use bellman::{
    bn256::Bn256,
    plonk::better_better_cs::proof::{Proof as PlonkProof, Proof},
};
use circuit_definitions::circuit_definitions::aux_layer::ZkSyncSnarkWrapperCircuit;
use ruint::aliases::{B160, U256};
use zk_ee::utils::Bytes32;
use zk_os_basic_system::system_implementation::system::{BatchOutput, BatchPublicInput};
use zksync_types::{
    commitment::{L1BatchWithMetadata, ZkosCommitment},
    H256,
};

pub mod commit;
pub mod methods;
pub mod structures;

// todo: will be refactored after zkos schema migration
// we'll probably compute the commitement earlier in the process and save in the DB

pub fn batch_public_input(
    prev_batch: &L1BatchWithMetadata,
    current_batch: &L1BatchWithMetadata,
) -> BatchPublicInput {
    let prev_commitment = ZkosCommitment::from(prev_batch);
    let current_commitment = ZkosCommitment::from(current_batch);

    BatchPublicInput {
        state_before: h256_to_bytes32(prev_commitment.state_commitment()),
        state_after: h256_to_bytes32(current_commitment.state_commitment()),
        batch_output: zkos_commitment_to_vm_batch_output(&current_commitment)
            .hash()
            .into(),
    }
}

pub fn zkos_commitment_to_vm_batch_output(commitment: &ZkosCommitment) -> BatchOutput {
    let (_, operator_da_input_header_hash) = commitment.calculate_operator_da_input();

    BatchOutput {
        chain_id: U256::from(commitment.chain_id),
        first_block_timestamp: commitment.block_timestamp,
        last_block_timestamp: commitment.block_timestamp,
        used_l2_da_validator_address: B160::default(),
        pubdata_commitment: h256_to_bytes32(operator_da_input_header_hash),
        number_of_layer_1_txs: U256::from(commitment.number_of_layer1_txs),
        priority_operations_hash: h256_to_bytes32(commitment.priority_operations_hash()),
        l2_logs_tree_root: h256_to_bytes32(commitment.l2_to_l1_logs_root_hash),
        upgrade_tx_hash: Bytes32::zero(),
    }
}

pub fn batch_output_hash_as_register_values(public_input: &BatchPublicInput) -> [u32; 8] {
    public_input
        .hash()
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().expect("Slice with incorrect length")))
        .collect::<Vec<u32>>()
        .try_into()
        .expect("Hash should be exactly 32 bytes long")
}

pub fn deserialize_snark_plank_proof(
    bytes: Vec<u8>,
) -> anyhow::Result<PlonkProof<Bn256, ZkSyncSnarkWrapperCircuit>> {
    let r: Proof<Bn256, ZkSyncSnarkWrapperCircuit> =
        bincode::deserialize(&bytes).context("cannot deserialize")?;
    Ok(r)
}

pub fn h256_to_bytes32(input: H256) -> Bytes32 {
    let mut new = Bytes32::zero();
    new.as_u8_array_mut().copy_from_slice(input.as_bytes());
    new
}

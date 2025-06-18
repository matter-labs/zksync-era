//! Different interfaces exposed by the `IExecutor.sol`.

use ruint::aliases::{B160, U256};
use zk_ee::utils::Bytes32;
use zk_os_basic_system::system_implementation::system::BatchOutput;
use zksync_types::{commitment::ZkosCommitment, H256};

pub mod commit;
pub mod methods;
pub mod structures;

// todo: will be refactored after zkos schema migration
// we'll probably compute the commitement earlier in the process and save in the DB
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
        dependency_roots_rolling_hash: h256_to_bytes32(commitment.dependency_roots_rolling_hash),
    }
}

pub fn h256_to_bytes32(input: H256) -> Bytes32 {
    let mut new = Bytes32::zero();
    new.as_u8_array_mut().copy_from_slice(input.as_bytes());
    new
}

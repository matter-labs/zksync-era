// Hide ugly auto-generated alloy structs outside of this module.
mod private {
    use zksync_types::commitment::ZkosCommitment;

    alloy::sol!(
        #[sol(all_derives)]
        "src/contracts/sol/IExecutor.sol"
    );
    // Copied from `PriorityTree.sol` as the entire file has imports that are unprocessable by `alloy::sol!`
    alloy::sol! {
        struct PriorityOpsBatchInfo {
            bytes32[] leftPath;
            bytes32[] rightPath;
            bytes32[] itemHashes;
        }
    }
    alloy::sol!(IZKChain, "src/contracts/artifacts/IZKChain.json");

    impl IExecutor::StoredBatchInfo {
        pub(crate) fn new(value: &ZkosCommitment, commitment: [u8; 32]) -> Self {
            Self::from((
                value.batch_number as u64,
                // For ZKsync OS batches we'll store here full state commitment
                alloy::primitives::FixedBytes::<32>::from(value.tree_root_hash.0),
                // Not used in Boojum OS, must be zero
                0u64,
                alloy::primitives::U256::from(value.number_of_layer1_txs),
                alloy::primitives::FixedBytes::<32>::from(value.priority_operations_hash().0),
                alloy::primitives::FixedBytes::<32>::from(value.l2_to_l1_logs_root_hash.0),
                // Not used in ZKsync OS, must be zero
                alloy::primitives::U256::from(0),
                // For ZKsync OS batches we'll store batch output hash here
                alloy::primitives::FixedBytes::<32>::from(commitment),
            ))
        }
    }

    impl From<&ZkosCommitment> for IExecutor::CommitBoojumOSBatchInfo {
        fn from(value: &ZkosCommitment) -> Self {
            let (operator_da_input, operator_da_input_header_hash) =
                value.calculate_operator_da_input();
            Self::from((
                value.batch_number as u64,
                alloy::primitives::FixedBytes::<32>::from(value.tree_root_hash.0),
                alloy::primitives::U256::from(value.number_of_layer1_txs),
                alloy::primitives::FixedBytes::<32>::from(value.priority_operations_hash().0),
                alloy::primitives::FixedBytes::<32>::from(value.l2_to_l1_logs_root_hash.0),
                alloy::primitives::Address::default(),
                alloy::primitives::FixedBytes::<32>::from(operator_da_input_header_hash.0),
                value.block_timestamp,
                value.block_timestamp,
                alloy::primitives::U256::from(value.chain_id),
                alloy::primitives::Bytes::from(operator_da_input),
            ))
        }
    }
}

pub use self::private::IZKChain::NewPriorityRequest;
use alloy::primitives::TxHash;

use self::private::{IExecutor, PriorityOpsBatchInfo};
use alloy::sol_types::{SolCall, SolValue};
use ruint::aliases::{B160, U256};
use zk_ee::utils::Bytes32;
use zk_os_basic_system::system_implementation::system::BatchOutput;
use zksync_types::commitment::{serialize_commitments, L1BatchWithMetadata, ZkosCommitment};
use zksync_types::l1::L1Tx;
use zksync_types::web3::keccak256;
use zksync_types::{L2ChainId, H256};
use zksync_zkos_vm_runner::zkos_conversions::h256_to_bytes32;

/// Current commitment encoding version as per protocol.
pub const SUPPORTED_ENCODING_VERSION: u8 = 0;

fn zkos_commitment_to_vm_batch_output(commitment: &ZkosCommitment) -> BatchOutput {
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


/// Builds a Solidity function call to `commitBatchesSharedBridge` as expected by `IExecutor.sol`.
///
/// Assumes system log verification and DA input verification are disabled.
pub fn commit_batches_shared_bridge_call(
    l2_chain_id: L2ChainId,
    last_committed_l1_batch: &ZkosCommitment,
    batch: &ZkosCommitment,
) -> impl SolCall {
    IExecutor::commitBatchesSharedBridgeCall::new((
        U256::from(l2_chain_id.as_u64()),
        U256::from(last_committed_l1_batch.batch_number + 1),
        U256::from(last_committed_l1_batch.batch_number + 1),
        commit_calldata(last_committed_l1_batch, batch).into(),
    ))
}

/// `commitBatchesSharedBridge` expects the rest of calldata to be of very specific form. This
/// function makes sure last committed batch and new batch are encoded correctly.
fn commit_calldata(
    last_committed_l1_batch: &ZkosCommitment,
    batch: &ZkosCommitment,
) -> Vec<u8> {
    let last_committed_batch_output: BatchOutput =
        zkos_commitment_to_vm_batch_output(last_committed_l1_batch);
    let last_commitment = last_committed_batch_output.hash();
    let stored_batch_info =
        IExecutor::StoredBatchInfo::new(last_committed_l1_batch, last_commitment);
    let last_batch_hash = H256(keccak256(stored_batch_info.abi_encode_params().as_slice()));
    tracing::info!(
        ?last_committed_l1_batch,
        state_commitment = ?last_committed_l1_batch.state_commitment(),
        last_commitment = ?H256::from(last_commitment),
        ?stored_batch_info,
        ?batch,
        ?last_batch_hash,
        "preparing commit calldata"
    );

    let commit_batch_info = IExecutor::CommitBoojumOSBatchInfo::from(batch);
    let encoded_data = (stored_batch_info, vec![commit_batch_info]).abi_encode_params();

    // Prefixed by current encoding version as expected by protocol
    [[SUPPORTED_ENCODING_VERSION].to_vec(), encoded_data]
        .concat()
        .to_vec()
}

// Hide ugly auto-generated alloy structs outside of this module.
mod private {
    use zksync_types::commitment::ZkosCommitment;

    alloy::sol! {
        // Taken from `IExecutor.sol`
        interface IExecutor {
            struct StoredBatchInfo {
                uint64 batchNumber;
                bytes32 batchHash;
                uint64 indexRepeatedStorageChanges;
                uint256 numberOfLayer1Txs;
                bytes32 priorityOperationsHash;
                bytes32 l2LogsTreeRoot;
                uint256 timestamp;
                bytes32 commitment;
            }

            struct CommitBoojumOSBatchInfo {
                uint64 batchNumber;
                bytes32 newStateCommitment;
                uint256 numberOfLayer1Txs;
                bytes32 priorityOperationsHash;
                bytes32 l2LogsTreeRoot;
                address l2DaValidator;
                bytes32 daCommitment;
                uint64 firstBlockTimestamp;
                uint64 lastBlockTimestamp;
                uint256 chainId;
                bytes operatorDAInput;
            }

            function commitBatchesSharedBridge(
                uint256 _chainId,
                uint256 _processFrom,
                uint256 _processTo,
                bytes calldata _commitData
            ) external;
        }

        // Taken from `PriorityTree.sol`
        struct PriorityOpsBatchInfo {
            bytes32[] leftPath;
            bytes32[] rightPath;
            bytes32[] itemHashes;
        }

        // Taken from `Messaging.sol`
        struct L2CanonicalTransaction {
            uint256 txType;
            uint256 from;
            uint256 to;
            uint256 gasLimit;
            uint256 gasPerPubdataByteLimit;
            uint256 maxFeePerGas;
            uint256 maxPriorityFeePerGas;
            uint256 paymaster;
            uint256 nonce;
            uint256 value;
            uint256[4] reserved;
            bytes data;
            bytes signature;
            uint256[] factoryDeps;
            bytes paymasterInput;
            bytes reservedDynamic;
        }

        // Taken from `IMailbox.sol`
        interface IMailbox {
            event NewPriorityRequest(
                uint256 txId,
                bytes32 txHash,
                uint64 expirationTimestamp,
                L2CanonicalTransaction transaction,
                bytes[] factoryDeps
            );
        }
    }

    impl IExecutor::StoredBatchInfo {
        pub(crate) fn new(value: &ZkosCommitment, commitment: [u8; 32]) -> Self {
            Self::from((
                value.batch_number as u64,
                // For ZKsync OS batches we'll store here full state commitment
                alloy::primitives::FixedBytes::<32>::from(value.state_commitment().0),
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
                alloy::primitives::FixedBytes::<32>::from(value.state_commitment().0),
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

pub use self::private::IMailbox::NewPriorityRequest;
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
        U256::from(batch.batch_number),
        commit_calldata(last_committed_l1_batch, batch).into(),
    ))
}

/// `commitBatchesSharedBridge` expects the rest of calldata to be of very specific form. This
/// function makes sure last committed batch and new batch are encoded correctly.
fn commit_calldata(
    last_commitment: &ZkosCommitment,
    new_commitment: &ZkosCommitment,
) -> Vec<u8> {
    let last_committed_batch_output: BatchOutput =
        zkos_commitment_to_vm_batch_output(last_commitment);
    let last_commitment_hash = last_committed_batch_output.hash();
    let stored_batch_info =
        IExecutor::StoredBatchInfo::new(last_commitment, last_commitment_hash);
    let last_batch_hash = H256(keccak256(stored_batch_info.abi_encode_params().as_slice()));
    let commit_batch_info = IExecutor::CommitBoojumOSBatchInfo::from(new_commitment);
    let encoded_data = (stored_batch_info, vec![commit_batch_info]).abi_encode_params();
    tracing::info!(
        ?last_batch_hash,
        last_batch_number = ?last_commitment.batch_number,
        new_batch_number = ?new_commitment.batch_number,
        "prepared commit calldata"
    );

    // Prefixed by current encoding version as expected by protocol
    [[SUPPORTED_ENCODING_VERSION].to_vec(), encoded_data]
        .concat()
        .to_vec()
}

use std::fmt::Debug;
use zksync_types::{commitment::{L1BatchCommitmentMode, L1BatchWithMetadata}, ethabi::{ParamType, Token}, pubdata_da::PubdataSendingMode, web3::{contract::Error as ContractError, keccak256}, ProtocolVersionId, H256, U256, Address};

use crate::{Tokenizable, Tokenize};
pub const PUBDATA_SOURCE_CALLDATA: u8 = 0;
pub const PUBDATA_SOURCE_BLOBS: u8 = 1;


// from https://github.com/matter-labs/era-contracts/blob/6ae03f3/l1-contracts/contracts/state-transition/chain-interfaces/IExecutor.sol#L106C1-L121C6
//
// struct CommitBoojumOSBatchInfo {
//     uint64 batchNumber;
//     bytes32 newStateCommitment;
//     // info about processed l1 txs, l2 to l1 logs and DA
//     uint256 numberOfLayer1Txs;
//     bytes32 priorityOperationsHash;
//     bytes32 l2LogsTreeRoot;
//     address l2DaValidator;
//     bytes32 daCommitment;
//     // sending used batch inputs to validate on the settlement layer
//     uint64 firstBlockTimestamp;
//     uint64 lastBlockTimestamp;
//     uint256 chainId;
//     // extra calldata to pass to da validator
//     bytes operatorDAInput;
// }
#[derive(Debug)]
pub struct CommitBoojumOSBatchInfo {
    pub batch_number: u32,
    pub new_state_commitment: H256,
    pub number_of_layer1_txs: U256,
    pub priority_operations_hash: H256,
    pub l2_logs_tree_root: H256,
    pub l2_da_validator: Address,
    pub da_commitment: H256,
    pub first_block_timestamp: u64,
    pub last_block_timestamp: u64,
    pub chain_id: U256,
    pub operator_da_input: Vec<u8>,
}

impl CommitBoojumOSBatchInfo {
    // todo: refactor `l1_batch_with_metadata` and corresponding table structure
    // currently it has fields for pre-boojum, boojum and zkos and most are not needed

    pub fn new(
        mode: L1BatchCommitmentMode,
        l1_batch_with_metadata: &L1BatchWithMetadata,
        pubdata_da: PubdataSendingMode,
    ) -> CommitBoojumOSBatchInfo {
        // other modes are not yet supported in ZK OS
        assert_eq!(mode, L1BatchCommitmentMode::Rollup);
        assert_eq!(pubdata_da, PubdataSendingMode::Calldata);

        // in zkos pubdata is returned from within the VM and saved by state keeper
        let pubdata = l1_batch_with_metadata
            .header
            .pubdata_input
            .clone()
            .expect("pubdata input must be present in ZK OS");

        let (operator_da_input, operator_da_input_header_hash) = calculate_operator_da_input(&pubdata);

        return Self {
            // set by state keeper when sealing block
            batch_number: l1_batch_with_metadata.header.number.0,
            // set by the tree (temporary set by state keeper from in-memory tree for now)
            new_state_commitment: l1_batch_with_metadata.metadata.root_hash,
            // set by state keeper when sealing block
            number_of_layer1_txs: U256::from(l1_batch_with_metadata.header.l1_tx_count),
            // uses header.priority_ops_onchain_data that is saved in state keeper
            priority_operations_hash: l1_batch_with_metadata
                .header
                .priority_ops_onchain_data_hash(),
            // set by state keeper when sealing block
            l2_logs_tree_root: l1_batch_with_metadata.metadata.l2_l1_merkle_root,
            // is currently ignored by smart contract
            l2_da_validator: Address::zero(),
            // computed above
            da_commitment: operator_da_input_header_hash.into(),
            // set by state keeper when sealing block
            first_block_timestamp: l1_batch_with_metadata.header.timestamp,
            // set by state keeper when sealing block
            last_block_timestamp: l1_batch_with_metadata.header.timestamp,
            // hardcoded for now - will be removed from smart contract later
            chain_id: U256::from(271),
            // computed above
            operator_da_input,
        };
    }
}

impl Tokenize for CommitBoojumOSBatchInfo {
    fn into_tokens(self) -> Vec<Token> {
        return vec![
            Token::Uint(self.batch_number.into()),
            Token::FixedBytes(self.new_state_commitment.as_bytes().to_vec()),
            Token::Uint(self.number_of_layer1_txs),
            Token::FixedBytes(self.priority_operations_hash.as_bytes().to_vec()),
            Token::FixedBytes(self.l2_logs_tree_root.as_bytes().to_vec()),
            Token::Address(self.l2_da_validator),
            Token::FixedBytes(self.da_commitment.as_bytes().to_vec()),
            Token::Uint(U256::from(self.first_block_timestamp)),
            Token::Uint(U256::from(self.last_block_timestamp)),
            Token::Uint(self.chain_id),
            Token::Bytes(self.operator_da_input),
        ];
    }
}
fn calculate_operator_da_input(pubdata: &Vec<u8>) -> (Vec<u8>, H256) {
    let mut operator_da_input: Vec<u8> = vec![];

    // reference for this header is taken from zk_ee: https://github.com/matter-labs/zk_ee/blob/ad-aggregation-program/aggregator/src/aggregation/da_commitment.rs#L27
    // consider reusing that code instead:
    //
    //             hasher.update([0u8; 32]); // we don't have to validate state diffs hash
    //                 hasher.update(Keccak256::digest(&pubdata)); // full pubdata keccak
    //                 hasher.update([1u8]); // with calldata we should provide 1 blob
    //                 hasher.update([0u8; 32]); // its hash will be ignored on the settlement layer
    //                 Ok(hasher.finalize().into())
    operator_da_input.extend(H256::zero().as_bytes());
    operator_da_input.extend(keccak256(&pubdata));
    operator_da_input.push(1);
    operator_da_input.extend(H256::zero().as_bytes());

    //     bytes32 daCommitment; - we compute hash of the first part of the operator_da_input (see above)
    let operator_da_input_header_hash: H256 = keccak256(&operator_da_input).into();

    operator_da_input.extend([PUBDATA_SOURCE_CALLDATA]);
    operator_da_input.extend(pubdata);
    // blob_commitment should be set to zero in ZK OS
    operator_da_input.extend(H256::zero().as_bytes());
    (operator_da_input, operator_da_input_header_hash)
}

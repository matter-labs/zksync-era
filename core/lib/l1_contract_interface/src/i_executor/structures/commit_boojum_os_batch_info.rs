use std::fmt::Debug;

use zksync_types::{commitment::ZkosCommitment, ethabi::Token, Address, H256, U256};

use crate::Tokenize;
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
#[derive(Debug, Clone)]
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
    pub fn new(commitment: &ZkosCommitment) -> CommitBoojumOSBatchInfo {
        let (operator_da_input, operator_da_input_header_hash) =
            commitment.calculate_operator_da_input();

        Self {
            batch_number: commitment.batch_number,
            new_state_commitment: commitment.state_commitment(),
            number_of_layer1_txs: commitment.number_of_layer1_txs.into(),
            priority_operations_hash: commitment.priority_operations_hash(),
            l2_logs_tree_root: commitment.l2_to_l1_logs_root_hash,
            l2_da_validator: Address::zero(),
            da_commitment: operator_da_input_header_hash,
            first_block_timestamp: commitment.block_timestamp,
            last_block_timestamp: commitment.block_timestamp,
            chain_id: U256::from(commitment.chain_id),
            operator_da_input,
        }
    }
}

impl Tokenize for CommitBoojumOSBatchInfo {
    fn into_tokens(self) -> Vec<Token> {
        vec![
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
        ]
    }
}

impl CommitBoojumOSBatchInfo {
    pub fn into_token(self) -> Token {
        Token::Array(vec![Token::Tuple(self.into_tokens())])
    }
}

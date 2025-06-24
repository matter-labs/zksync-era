use blake2::{Blake2s256, Digest};
use zksync_basic_types::{web3::keccak256, L2ChainId};

use crate::{
    commitment::L1BatchWithMetadata, priority_op_onchain_data::PriorityOpOnchainData, H256,
};

pub const PUBDATA_SOURCE_CALLDATA: u8 = 0;
pub const PUBDATA_SOURCE_BLOBS: u8 = 1;
#[derive(Debug)]
// Only has a base set of values - others are computed - see `Impl`
// Names are taken from IExecutor.sol - where possible
// After DB schema zkos migration, May replace L1BatchHeader
pub struct ZkosCommitment {
    // set by state keeper
    pub batch_number: u32,
    // set by state keeper
    // note: will probably be replaced with (first_block_timestamp, last_block_timestamp) when aggregating blocks
    pub block_timestamp: u64,
    // set by the tree (temporary set by state keeper from in-memory tree for now)
    pub tree_root_hash: H256,
    pub tree_next_free_index: u64,

    // set by state keeper
    pub number_of_layer1_txs: u16,
    pub number_of_layer2_txs: u16,

    // set by state keeper
    pub priority_ops_onchain_data: Vec<PriorityOpOnchainData>,
    // set by state keeper (computed using MiniMerkleTree)
    pub l2_to_l1_logs_root_hash: H256,
    // not implemented - and currently ignored by smart contract
    // pub l2_da_validator: Address,

    // set by state keeper
    // todo: potentially large, double check
    pub pubdata: Vec<u8>,

    // todo: hardcoded for now - will be removed from smart contract later
    pub chain_id: u32,
}

impl ZkosCommitment {
    // copied from `L1BatchHeader` struct
    pub fn priority_operations_hash(&self) -> H256 {
        let mut rolling_hash: H256 = keccak256(&[]).into();
        for onchain_data in &self.priority_ops_onchain_data {
            let mut preimage = Vec::new();
            preimage.extend(rolling_hash.as_bytes());
            preimage.extend(onchain_data.onchain_data_hash.as_bytes());

            rolling_hash = keccak256(&preimage).into();
        }

        rolling_hash
    }

    pub fn state_commitment(&self) -> H256 {
        let mut hasher = Blake2s256::new();
        hasher.update(self.tree_root_hash.as_bytes()); // as_u8_ref in ruint
        hasher.update(self.tree_next_free_index.to_be_bytes()); // as_be_bytes in ruint

        // return the final hash
        H256::from_slice(&hasher.finalize())
    }

    // returns the operator da input and its header hash
    //  (operator_da_input, operator_da_input_header_hash)
    // used in CommitBatchData onchain
    pub fn calculate_operator_da_input(&self) -> (Vec<u8>, H256) {
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
        operator_da_input.extend(keccak256(&self.pubdata));
        operator_da_input.push(1);
        operator_da_input.extend(H256::zero().as_bytes());

        //     bytes32 daCommitment; - we compute hash of the first part of the operator_da_input (see above)
        let operator_da_input_header_hash: H256 = keccak256(&operator_da_input).into();

        operator_da_input.extend([PUBDATA_SOURCE_CALLDATA]);
        operator_da_input.extend(&self.pubdata);
        // blob_commitment should be set to zero in ZK OS
        operator_da_input.extend(H256::zero().as_bytes());
        (operator_da_input, operator_da_input_header_hash)
    }
}

impl ZkosCommitment {
    pub fn new(batch: &L1BatchWithMetadata, chain_id: L2ChainId) -> Self {
        // We instantiate the l2 chain id only after the first block, the zero batch is always 271
        let chain_id = if batch.header.number.0 == 0 {
            L2ChainId::default()
        } else {
            chain_id
        };

        ZkosCommitment {
            batch_number: batch.header.number.0,
            block_timestamp: batch.header.timestamp,
            tree_root_hash: batch.metadata.root_hash,
            tree_next_free_index: batch.metadata.rollup_last_leaf_index + 1,
            number_of_layer1_txs: batch.header.l1_tx_count,
            number_of_layer2_txs: batch.header.l2_tx_count,
            priority_ops_onchain_data: batch.header.priority_ops_onchain_data.clone(),
            l2_to_l1_logs_root_hash: batch.metadata.l2_l1_merkle_root,
            pubdata: batch.header.pubdata_input.clone().unwrap(),
            chain_id: chain_id.as_u64() as u32,
        }
    }
}

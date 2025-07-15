use anyhow::Context as _;
use zk_os_basic_system::system_implementation::system::BatchOutput;
use zksync_types::{
    commitment::{L1BatchWithMetadata, ZkosCommitment},
    ethabi::{self, ParamType, Token},
    parse_h256, web3,
    web3::contract::Error as ContractError,
    ProtocolVersionId, H256, U256,
};

use crate::{zkos_commitment_to_vm_batch_output, Tokenizable};

// https://github.com/matter-labs/era-contracts/blob/ad-for-rb-only-l1/l1-contracts/contracts/state-transition/chain-interfaces/IExecutor.sol#L64-L73
// struct StoredBatchInfo {
//         uint64 batchNumber;
//         bytes32 batchHash; // For Boojum OS batches we'll store here full state commitment
//         uint64 indexRepeatedStorageChanges; // For Boojum OS not used, 0
//         uint256 numberOfLayer1Txs;
//         bytes32 priorityOperationsHash;
//         bytes32 l2LogsTreeRoot;
//         uint256 timestamp; // For Boojum OS not used, 0
//         bytes32 commitment;// For Boojum OS batches we'll store public input here
//     }
//
#[derive(Debug, Clone, PartialEq)]
pub struct StoredBatchInfo {
    pub batch_number: u64,
    // in zk_os, state commitement is used here:
    pub batch_hash: H256,
    pub index_repeated_storage_changes: u64, // not used in Boojum OS, must be zero
    pub number_of_layer1_txs: U256,
    pub priority_operations_hash: H256,
    pub dependency_roots_rolling_hash: H256,
    pub l2_logs_tree_root: H256,
    pub timestamp: U256,
    pub commitment: H256,
}

impl StoredBatchInfo {
    pub fn schema() -> ParamType {
        ParamType::Tuple(vec![
            ParamType::Uint(64),       // `batch_number`
            ParamType::FixedBytes(32), // `batch_hash`
            ParamType::Uint(64),       // `index_repeated_storage_changes`
            ParamType::Uint(256),      // `number_of_layer1_txs`
            ParamType::FixedBytes(32), // `priority_operations_hash`
            ParamType::FixedBytes(32), // `dependency_roots_rolling_hash`
            ParamType::FixedBytes(32), // `l2_logs_tree_root`
            ParamType::Uint(256),      // `timestamp`
            ParamType::FixedBytes(32), // `commitment`
        ])
    }

    /// Encodes the struct into RLP.
    pub fn encode(&self) -> Vec<u8> {
        ethabi::encode(&[self.clone().into_token()])
    }

    /// Decodes the struct from RLP.
    pub fn decode(rlp: &[u8]) -> anyhow::Result<Self> {
        let [token] = ethabi::decode_whole(&[Self::schema()], rlp)?
            .try_into()
            .unwrap();
        Ok(Self::from_token(token)?)
    }

    /// `_hashStoredBatchInfo` from `Executor.sol`.
    pub fn hash(&self) -> H256 {
        H256(web3::keccak256(&self.encode()))
    }

    pub fn into_token_with_protocol_version(self, protocol_version: ProtocolVersionId) -> Token {
        if protocol_version.is_pre_interop() {
            Token::Tuple(vec![
                Token::Uint(self.batch_number.into()),
                Token::FixedBytes(self.batch_hash.as_bytes().to_vec()),
                Token::Uint(self.index_repeated_storage_changes.into()),
                Token::Uint(self.number_of_layer1_txs),
                Token::FixedBytes(self.priority_operations_hash.as_bytes().to_vec()),
                Token::FixedBytes(self.l2_logs_tree_root.as_bytes().to_vec()),
                Token::Uint(self.timestamp),
                Token::FixedBytes(self.commitment.as_bytes().to_vec()),
            ])
        } else {
            Token::Tuple(vec![
                Token::Uint(self.batch_number.into()),
                Token::FixedBytes(self.batch_hash.as_bytes().to_vec()),
                Token::Uint(self.index_repeated_storage_changes.into()),
                Token::Uint(self.number_of_layer1_txs),
                Token::FixedBytes(self.priority_operations_hash.as_bytes().to_vec()),
                Token::FixedBytes(self.dependency_roots_rolling_hash.as_bytes().to_vec()),
                Token::FixedBytes(self.l2_logs_tree_root.as_bytes().to_vec()),
                Token::Uint(self.timestamp),
                Token::FixedBytes(self.commitment.as_bytes().to_vec()),
            ])
        }
    }
}

// todo when L1BatchWithMetadata is refactored, we should convert it to this struct directly
impl StoredBatchInfo {
    pub fn new(batch: &ZkosCommitment, commitment: [u8; 32]) -> Self {
        Self {
            batch_number: batch.batch_number.into(),
            batch_hash: batch.state_commitment(),
            index_repeated_storage_changes: 0, // not used in Boojum OS, must be zero
            number_of_layer1_txs: batch.number_of_layer1_txs.into(),
            priority_operations_hash: batch.priority_operations_hash(),
            dependency_roots_rolling_hash: batch.dependency_roots_rolling_hash,
            l2_logs_tree_root: batch.l2_to_l1_logs_root_hash,
            timestamp: 0.into(),
            commitment: commitment.into(),
        }
    }
}

// todo: this conversion is only used by legacy methods - it will not work correctly in zkos
// commitment is not computed correctly here
impl From<&L1BatchWithMetadata> for StoredBatchInfo {
    fn from(x: &L1BatchWithMetadata) -> Self {
        let last_block_commitment: ZkosCommitment = x.into();
        let batch_output: BatchOutput = zkos_commitment_to_vm_batch_output(&last_block_commitment);
        let stored_batch_info = StoredBatchInfo::new(&last_block_commitment, batch_output.hash());
        stored_batch_info
    }
}

impl Tokenizable for StoredBatchInfo {
    fn from_token(token: Token) -> Result<Self, ContractError> {
        (|| {
            let [
                Token::Uint(batch_number),
                Token::FixedBytes(batch_hash),
                Token::Uint(index_repeated_storage_changes),
                Token::Uint(number_of_layer1_txs),
                Token::FixedBytes(priority_operations_hash),
                Token::FixedBytes(dependency_roots_rolling_hash),
                Token::FixedBytes(l2_logs_tree_root),
                Token::Uint(timestamp),
                Token::FixedBytes(commitment),
            ] : [Token;9] = token
                .into_tuple().context("not a tuple")?
                .try_into().ok().context("bad length")?
            else { anyhow::bail!("bad format") };
            Ok(Self {
                batch_number: batch_number
                    .try_into()
                    .ok()
                    .context("overflow")
                    .context("batch_number")?,
                batch_hash: parse_h256(&batch_hash).context("batch_hash")?,
                index_repeated_storage_changes: index_repeated_storage_changes
                    .try_into()
                    .ok()
                    .context("overflow")
                    .context("index_repeated_storage_changes")?,
                number_of_layer1_txs,
                priority_operations_hash: parse_h256(&priority_operations_hash)
                    .context("priority_operations_hash")?,
                dependency_roots_rolling_hash: parse_h256(&dependency_roots_rolling_hash)
                    .context("dependency_roots_rolling_hash")?,
                l2_logs_tree_root: parse_h256(&l2_logs_tree_root).context("l2_logs_tree_root")?,
                timestamp,
                commitment: parse_h256(&commitment).context("commitment")?,
            })
        })()
        .map_err(|err| ContractError::InvalidOutputType(format!("{err:#}")))
    }

    fn into_token(self) -> Token {
        self.into_token_with_protocol_version(ProtocolVersionId::latest())
    }
}

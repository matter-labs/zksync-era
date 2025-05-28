use anyhow::Context;
use chrono::{DateTime, Utc};
use derive_more::Display;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use serde_with::{hex::Hex, serde_as};
use zksync_basic_types::{
    commitment::PubdataType,
    settlement::SettlementLayer,
    web3::{AccessList, Bytes, Index},
    Bloom, L1BatchNumber, SLChainId, H160, H256, H64, U256, U64,
};
use zksync_contracts::BaseSystemContractsHashes;

pub use crate::transaction_request::{
    Eip712Meta, SerializationTransactionError, TransactionRequest,
};
use crate::{
    debug_flat_call::{DebugCallFlat, ResultDebugCallFlat},
    eth_sender::EthTxFinalityStatus,
    protocol_version::L1VerifierConfig,
    server_notification::{GatewayMigrationNotification, GatewayMigrationState},
    tee_types::TeeType,
    Address, L2BlockNumber, ProtocolVersionId,
};

pub mod en;
pub mod state_override;

/// Block Number
#[derive(Copy, Clone, Debug, PartialEq, Display)]
pub enum BlockNumber {
    /// Alias for BlockNumber::Latest.
    Committed,
    /// Last block that was finalized on L1.
    Finalized,
    /// Last block that was fast finalized on L1.
    FastFinalized,
    /// Latest sealed block
    Latest,
    /// Last block that was committed on L1
    L1Committed,
    /// Earliest block (genesis)
    Earliest,
    /// Latest block (may be the block that is currently open).
    Pending,
    /// Block by number from canon chain
    Number(U64),
}

impl<T: Into<U64>> From<T> for BlockNumber {
    fn from(num: T) -> Self {
        BlockNumber::Number(num.into())
    }
}

impl Serialize for BlockNumber {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match *self {
            BlockNumber::Number(ref x) => serializer.serialize_str(&format!("0x{:x}", x)),
            BlockNumber::Committed => serializer.serialize_str("committed"),
            BlockNumber::Finalized => serializer.serialize_str("finalized"),
            BlockNumber::Latest => serializer.serialize_str("latest"),
            BlockNumber::L1Committed => serializer.serialize_str("l1_committed"),
            BlockNumber::Earliest => serializer.serialize_str("earliest"),
            BlockNumber::Pending => serializer.serialize_str("pending"),
            BlockNumber::FastFinalized => serializer.serialize_str("fast_finalized"),
        }
    }
}

impl<'de> Deserialize<'de> for BlockNumber {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = BlockNumber;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("A block number or one of the supported aliases")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                let result = match value {
                    "committed" => BlockNumber::Committed,
                    "finalized" => BlockNumber::Finalized,
                    "latest" => BlockNumber::Latest,
                    "l1_committed" => BlockNumber::L1Committed,
                    "earliest" => BlockNumber::Earliest,
                    // For zksync safe is l1 committed. Real chances of revert are very low.
                    "safe" => BlockNumber::L1Committed,
                    "pending" => BlockNumber::Pending,
                    "fast_finalized" => BlockNumber::FastFinalized,
                    num => {
                        let number =
                            U64::deserialize(de::value::BorrowedStrDeserializer::new(num))?;
                        BlockNumber::Number(number)
                    }
                };

                Ok(result)
            }
        }
        deserializer.deserialize_str(V)
    }
}

/// Block unified identifier in terms of ZKsync
///
/// This is an utility structure that cannot be (de)serialized, it has to be created manually.
/// The reason is because Web3 API provides multiple methods for referring block either by hash or number,
/// and with such an ID it will be possible to avoid a lot of boilerplate.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize, Display)]
#[serde(untagged)]
pub enum BlockId {
    /// By Hash
    Hash(H256),
    /// By Number
    Number(BlockNumber),
}

impl BlockId {
    /// Extract block's id variant name.
    pub fn extract_block_tag(&self) -> String {
        match self {
            BlockId::Number(block_number) => block_number.to_string(),
            BlockId::Hash(_) => "hash".to_string(),
        }
    }
}

/// Helper struct for EIP-1898.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockNumberObject {
    pub block_number: BlockNumber,
}

/// Helper struct for EIP-1898.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockHashObject {
    pub block_hash: H256,
}

/// Helper enum for EIP-1898.
/// Should be used for `block` parameters in web3 JSON RPC methods that implement EIP-1898.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BlockIdVariant {
    BlockNumber(BlockNumber),
    BlockNumberObject(BlockNumberObject),
    BlockHashObject(BlockHashObject),
}

impl From<BlockIdVariant> for BlockId {
    fn from(value: BlockIdVariant) -> BlockId {
        match value {
            BlockIdVariant::BlockNumber(number) => BlockId::Number(number),
            BlockIdVariant::BlockNumberObject(number_object) => {
                BlockId::Number(number_object.block_number)
            }
            BlockIdVariant::BlockHashObject(hash_object) => BlockId::Hash(hash_object.block_hash),
        }
    }
}

/// Transaction variant
///
/// Utility structure. Some Web3 API methods have to return a block with a list of either full
/// transaction objects or just their hashes.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TransactionVariant {
    Full(Transaction),
    Hash(H256),
}

/// Transaction Identifier
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TransactionId {
    /// By hash
    Hash(H256),
    /// By block and index
    Block(BlockId, Index),
}

impl From<H256> for TransactionId {
    fn from(hash: H256) -> Self {
        TransactionId::Hash(hash)
    }
}

/// Merkle root target for interop log proofs
#[derive(Copy, Clone, Debug, PartialEq, Display)]
pub enum LogProofTarget {
    // L2's ChainBatchRoot
    Chain,
    // Gateway's MessageRoot
    GatewayMessageRoot,
    // GGateway's ChainBatchRoot. Fallback behaviour, used for withdrawals
    GatewayChainBatchRoot,
}

impl Serialize for LogProofTarget {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match *self {
            LogProofTarget::Chain => serializer.serialize_str("chain"),
            LogProofTarget::GatewayMessageRoot => serializer.serialize_str("gw_message_root"),
            _ => serializer.serialize_str("gw_chain_batch_root"),
        }
    }
}

impl<'de> Deserialize<'de> for LogProofTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = LogProofTarget;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("One of the supported aliases")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                let result = match value {
                    "chain" => LogProofTarget::Chain,
                    "gw_message_root" => LogProofTarget::GatewayMessageRoot,
                    _ => LogProofTarget::GatewayChainBatchRoot,
                };

                Ok(result)
            }
        }
        deserializer.deserialize_str(V)
    }
}

/// A struct with the proof for the L2->L1 log in a specific block.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct L2ToL1LogProof {
    /// The merkle path for the leaf.
    pub proof: Vec<H256>,
    /// The id of the leaf in a tree.
    pub id: u32,
    /// The root of the tree.
    pub root: H256,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChainAggProof {
    pub chain_id_leaf_proof: Vec<H256>,
    pub chain_id_leaf_proof_mask: u64,
}

/// A struct with the two default bridge contracts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeAddresses {
    pub l1_shared_default_bridge: Option<Address>,
    pub l2_shared_default_bridge: Option<Address>,
    pub l1_erc20_default_bridge: Option<Address>,
    pub l2_erc20_default_bridge: Option<Address>,
    pub l1_weth_bridge: Option<Address>,
    pub l2_weth_bridge: Option<Address>,
    pub l2_legacy_shared_bridge: Option<Address>,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionReceipt {
    /// Transaction hash.
    #[serde(rename = "transactionHash")]
    pub transaction_hash: H256,
    /// Index within the block.
    #[serde(rename = "transactionIndex")]
    pub transaction_index: Index,
    /// Hash of the block this transaction was included within.
    #[serde(rename = "blockHash")]
    pub block_hash: H256,
    /// Number of the L2 block this transaction was included within.
    #[serde(rename = "blockNumber")]
    pub block_number: U64,
    /// Index of transaction in l1 batch
    #[serde(rename = "l1BatchTxIndex")]
    pub l1_batch_tx_index: Option<Index>,
    /// Number of the l1 batch this transaction was included within.
    #[serde(rename = "l1BatchNumber")]
    pub l1_batch_number: Option<U64>,
    /// Sender
    /// Note: default address if the client did not return this value
    /// (maintains backwards compatibility for <= 0.7.0 when this field was missing)
    #[serde(default)]
    pub from: Address,
    /// Recipient (None when contract creation)
    /// Note: Also `None` if the client did not return this value
    /// (maintains backwards compatibility for <= 0.7.0 when this field was missing)
    #[serde(default)]
    pub to: Option<Address>,
    /// Cumulative gas used within the block after this was executed.
    #[serde(rename = "cumulativeGasUsed")]
    pub cumulative_gas_used: U256,
    /// Gas used by this transaction alone.
    ///
    /// Gas used is `None` if the the client is running in light client mode.
    #[serde(rename = "gasUsed")]
    pub gas_used: Option<U256>,
    /// Contract address created, or `None` if not a deployment.
    #[serde(rename = "contractAddress")]
    pub contract_address: Option<Address>,
    /// Logs generated within this transaction.
    pub logs: Vec<Log>,
    /// L2 to L1 logs generated within this transaction.
    #[serde(rename = "l2ToL1Logs")]
    pub l2_to_l1_logs: Vec<L2ToL1Log>,
    /// Status: either 1 (success) or 0 (failure).
    pub status: U64,
    /// Logs bloom
    #[serde(rename = "logsBloom")]
    pub logs_bloom: Bloom,
    /// Transaction type, Some(1) for AccessList transaction, None for Legacy
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub transaction_type: Option<U64>,
    /// Effective gas price
    #[serde(rename = "effectiveGasPrice")]
    pub effective_gas_price: Option<U256>,
}

/// The block type returned from RPC calls.
/// This is generic over a `TX` type.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Block<TX> {
    /// Hash of the block
    pub hash: H256,
    /// Hash of the parent
    #[serde(rename = "parentHash")]
    pub parent_hash: H256,
    /// Hash of the uncles
    #[serde(rename = "sha3Uncles")]
    pub uncles_hash: H256,
    /// Miner / author's address
    #[serde(rename = "miner", default, deserialize_with = "null_to_default")]
    pub author: H160,
    /// State root hash
    #[serde(rename = "stateRoot")]
    pub state_root: H256,
    /// Transactions root hash
    #[serde(rename = "transactionsRoot")]
    pub transactions_root: H256,
    /// Transactions receipts root hash
    #[serde(rename = "receiptsRoot")]
    pub receipts_root: H256,
    /// Block number
    pub number: U64,
    /// L1 batch number the block is included in
    #[serde(rename = "l1BatchNumber")]
    pub l1_batch_number: Option<U64>,
    /// Gas Used
    #[serde(rename = "gasUsed")]
    pub gas_used: U256,
    /// Gas Limit
    #[serde(rename = "gasLimit")]
    pub gas_limit: U256,
    /// Base fee per unit of gas
    #[serde(rename = "baseFeePerGas")]
    pub base_fee_per_gas: U256,
    /// Extra data
    #[serde(rename = "extraData")]
    pub extra_data: Bytes,
    /// Logs bloom
    #[serde(rename = "logsBloom")]
    pub logs_bloom: Bloom,
    /// Timestamp
    pub timestamp: U256,
    /// Timestamp of the l1 batch this L2 block was included within
    #[serde(rename = "l1BatchTimestamp")]
    pub l1_batch_timestamp: Option<U256>,
    /// Difficulty
    pub difficulty: U256,
    /// Total difficulty
    #[serde(rename = "totalDifficulty")]
    pub total_difficulty: U256,
    /// Seal fields
    #[serde(default, rename = "sealFields")]
    pub seal_fields: Vec<Bytes>,
    /// Uncles' hashes
    pub uncles: Vec<H256>,
    /// Transactions
    pub transactions: Vec<TX>,
    /// Size in bytes
    pub size: U256,
    /// Mix Hash
    #[serde(rename = "mixHash")]
    pub mix_hash: H256,
    /// Nonce
    pub nonce: H64,
}

// We want to implement `Default` for all `TX`s, not only for `TX: Default`, hence this manual impl.
impl<TX> Default for Block<TX> {
    fn default() -> Self {
        Self {
            hash: H256::default(),
            parent_hash: H256::default(),
            uncles_hash: H256::default(),
            author: Address::default(),
            state_root: H256::default(),
            transactions_root: H256::default(),
            receipts_root: H256::default(),
            number: U64::default(),
            l1_batch_number: None,
            gas_used: U256::default(),
            gas_limit: U256::default(),
            base_fee_per_gas: U256::default(),
            extra_data: Bytes::default(),
            logs_bloom: Bloom::default(),
            timestamp: U256::default(),
            l1_batch_timestamp: None,
            difficulty: U256::default(),
            total_difficulty: U256::default(),
            seal_fields: vec![],
            uncles: vec![],
            transactions: vec![],
            size: U256::default(),
            mix_hash: H256::default(),
            nonce: H64::default(),
        }
    }
}

impl<TX> Block<TX> {
    pub fn with_transactions<U>(self, transactions: Vec<U>) -> Block<U> {
        Block {
            hash: self.hash,
            parent_hash: self.parent_hash,
            uncles_hash: self.uncles_hash,
            author: self.author,
            state_root: self.state_root,
            transactions_root: self.transactions_root,
            receipts_root: self.receipts_root,
            number: self.number,
            l1_batch_number: self.l1_batch_number,
            gas_used: self.gas_used,
            gas_limit: self.gas_limit,
            base_fee_per_gas: self.base_fee_per_gas,
            extra_data: self.extra_data,
            logs_bloom: self.logs_bloom,
            timestamp: self.timestamp,
            l1_batch_timestamp: self.l1_batch_timestamp,
            difficulty: self.difficulty,
            total_difficulty: self.total_difficulty,
            seal_fields: self.seal_fields,
            uncles: self.uncles,
            transactions,
            size: self.size,
            mix_hash: self.mix_hash,
            nonce: self.nonce,
        }
    }
}

fn null_to_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: Default + Deserialize<'de>,
    D: Deserializer<'de>,
{
    let option = Option::deserialize(deserializer)?;
    Ok(option.unwrap_or_default())
}

/// A log produced by a transaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Log {
    /// H160
    pub address: H160,
    /// Topics
    pub topics: Vec<H256>,
    /// Data
    pub data: Bytes,
    /// Block Hash
    #[serde(rename = "blockHash")]
    pub block_hash: Option<H256>,
    /// Block Number
    #[serde(rename = "blockNumber")]
    pub block_number: Option<U64>,
    /// L1 batch number the log is included in.
    #[serde(rename = "l1BatchNumber")]
    pub l1_batch_number: Option<U64>,
    /// Transaction Hash
    #[serde(rename = "transactionHash")]
    pub transaction_hash: Option<H256>,
    /// Transaction Index
    #[serde(rename = "transactionIndex")]
    pub transaction_index: Option<Index>,
    /// Log Index in Block
    #[serde(rename = "logIndex")]
    pub log_index: Option<U256>,
    /// Log Index in Transaction
    #[serde(rename = "transactionLogIndex")]
    pub transaction_log_index: Option<U256>,
    /// Log Type
    #[serde(rename = "logType")]
    pub log_type: Option<String>,
    /// Removed
    pub removed: Option<bool>,
    /// L2 block timestamp
    #[serde(rename = "blockTimestamp")]
    pub block_timestamp: Option<U64>,
}

impl Log {
    /// Returns true if the log has been removed.
    pub fn is_removed(&self) -> bool {
        if let Some(val_removed) = self.removed {
            return val_removed;
        }

        if let Some(ref val_log_type) = self.log_type {
            if val_log_type == "removed" {
                return true;
            }
        }
        false
    }
}

impl From<Log> for zksync_basic_types::web3::Log {
    fn from(log: Log) -> Self {
        zksync_basic_types::web3::Log {
            address: log.address,
            topics: log.topics,
            data: log.data,
            block_hash: log.block_hash,
            block_number: log.block_number,
            transaction_hash: log.transaction_hash,
            transaction_index: log.transaction_index,
            log_index: log.log_index,
            transaction_log_index: log.transaction_log_index,
            log_type: log.log_type,
            removed: log.removed,
            block_timestamp: log.block_timestamp,
        }
    }
}

impl From<zksync_basic_types::web3::Log> for Log {
    fn from(log: zksync_basic_types::web3::Log) -> Self {
        Log {
            address: log.address,
            topics: log.topics,
            data: log.data,
            block_hash: log.block_hash,
            block_number: log.block_number,
            transaction_hash: log.transaction_hash,
            transaction_index: log.transaction_index,
            log_index: log.log_index,
            transaction_log_index: log.transaction_log_index,
            log_type: log.log_type,
            removed: log.removed,
            block_timestamp: log.block_timestamp,
            l1_batch_number: None,
        }
    }
}

/// A log produced by a transaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct L2ToL1Log {
    pub block_hash: Option<H256>,
    pub block_number: U64,
    pub l1_batch_number: Option<U64>,
    pub log_index: U256,
    pub transaction_index: Index,
    pub transaction_hash: H256,
    pub transaction_log_index: U256,
    pub tx_index_in_l1_batch: Option<U64>,
    pub shard_id: U64,
    pub is_service: bool,
    pub sender: Address,
    pub key: H256,
    pub value: H256,
}

/// Description of a Transaction, pending or in the chain.
#[derive(Debug, Default, Clone, PartialEq, Deserialize, Serialize)]
pub struct Transaction {
    /// Hash
    pub hash: H256,
    /// Nonce
    pub nonce: U256,
    /// Block hash. None when pending.
    #[serde(rename = "blockHash")]
    pub block_hash: Option<H256>,
    /// Block number. None when pending.
    #[serde(rename = "blockNumber")]
    pub block_number: Option<U64>,
    /// Transaction Index. None when pending.
    #[serde(rename = "transactionIndex")]
    pub transaction_index: Option<Index>,
    /// Sender
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Address>,
    /// Recipient (None when contract creation)
    pub to: Option<Address>,
    /// Transferred value
    pub value: U256,
    /// Gas Price
    #[serde(rename = "gasPrice")]
    pub gas_price: Option<U256>,
    /// Gas amount
    pub gas: U256,
    /// Input data
    pub input: Bytes,
    /// The parity (0 for even, 1 for odd) of the y-value of the secp256k1 signature
    #[serde(rename = "yParity", default, skip_serializing_if = "Option::is_none")]
    pub y_parity: Option<U64>,
    /// ECDSA recovery id
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub v: Option<U64>,
    /// ECDSA signature r, 32 bytes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r: Option<U256>,
    /// ECDSA signature s, 32 bytes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s: Option<U256>,
    /// Raw transaction data
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<Bytes>,
    /// Transaction type, Some(1) for AccessList transaction, None for Legacy
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub transaction_type: Option<U64>,
    /// Access list
    #[serde(
        rename = "accessList",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub access_list: Option<AccessList>,
    /// Max fee per gas
    #[serde(rename = "maxFeePerGas", skip_serializing_if = "Option::is_none")]
    pub max_fee_per_gas: Option<U256>,
    /// Miner bribe
    #[serde(
        rename = "maxPriorityFeePerGas",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_priority_fee_per_gas: Option<U256>,
    /// Id of the current chain
    #[serde(rename = "chainId")]
    pub chain_id: U256,
    /// Number of the l1 batch this transaction was included within.
    #[serde(
        rename = "l1BatchNumber",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub l1_batch_number: Option<U64>,
    /// Index of transaction in l1 batch
    #[serde(
        rename = "l1BatchTxIndex",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub l1_batch_tx_index: Option<U64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum TransactionStatus {
    Pending,
    Included,
    FastFinalized,
    Verified,
    Failed,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDetails {
    pub is_l1_originated: bool,
    pub status: TransactionStatus,
    pub fee: U256,
    pub gas_per_pubdata: U256,
    pub initiator_address: Address,
    pub received_at: DateTime<Utc>,
    pub eth_commit_tx_hash: Option<H256>,
    pub eth_prove_tx_hash: Option<H256>,
    pub eth_execute_tx_hash: Option<H256>,
}

#[derive(Debug, Clone)]
pub struct GetLogsFilter {
    pub from_block: L2BlockNumber,
    pub to_block: L2BlockNumber,
    pub addresses: Vec<Address>,
    pub topics: Vec<(u32, Vec<H256>)>,
}

/// Result of debugging block
/// For some reasons geth returns result as {result: DebugCall}
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResultDebugCall {
    pub result: DebugCall,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum DebugCallType {
    #[default]
    Call,
    DelegateCall,
    Create,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugCall {
    pub r#type: DebugCallType,
    pub from: Address,
    pub to: Address,
    pub gas: U256,
    pub gas_used: U256,
    pub value: U256,
    pub output: Bytes,
    pub input: Bytes,
    pub error: Option<String>,
    pub revert_reason: Option<String>,
    pub calls: Vec<DebugCall>,
}

// TODO: remove in favour of `ProtocolVersionInfo` once all ENs have been upgraded.
#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct ProtocolVersion {
    /// Minor version of the protocol
    #[deprecated]
    pub version_id: Option<u16>,
    /// Minor version of the protocol
    #[serde(rename = "minorVersion")]
    pub minor_version: Option<u16>,
    /// Timestamp at which upgrade should be performed
    pub timestamp: u64,
    /// Verifier configuration
    #[deprecated]
    pub verification_keys_hashes: Option<L1VerifierConfig>,
    /// Hashes of base system contracts (bootloader, default account and evm emulator)
    #[deprecated]
    pub base_system_contracts: Option<BaseSystemContractsHashes>,
    /// Bootloader code hash
    #[serde(rename = "bootloaderCodeHash")]
    pub bootloader_code_hash: Option<H256>,
    /// Default account code hash
    #[serde(rename = "defaultAccountCodeHash")]
    pub default_account_code_hash: Option<H256>,
    /// EVM emulator code hash
    #[serde(rename = "evmSimulatorCodeHash")]
    pub evm_emulator_code_hash: Option<H256>,
    /// L2 Upgrade transaction hash
    #[deprecated]
    pub l2_system_upgrade_tx_hash: Option<H256>,
    /// L2 Upgrade transaction hash
    #[serde(rename = "l2SystemUpgradeTxHash")]
    pub l2_system_upgrade_tx_hash_new: Option<H256>,
}

#[allow(deprecated)]
impl ProtocolVersion {
    pub fn new(
        minor_version: u16,
        timestamp: u64,
        bootloader_code_hash: H256,
        default_account_code_hash: H256,
        evm_emulator_code_hash: Option<H256>,
        l2_system_upgrade_tx_hash: Option<H256>,
    ) -> Self {
        Self {
            version_id: Some(minor_version),
            minor_version: Some(minor_version),
            timestamp,
            verification_keys_hashes: Some(Default::default()),
            base_system_contracts: Some(BaseSystemContractsHashes {
                bootloader: bootloader_code_hash,
                default_aa: default_account_code_hash,
                evm_emulator: evm_emulator_code_hash,
            }),
            bootloader_code_hash: Some(bootloader_code_hash),
            default_account_code_hash: Some(default_account_code_hash),
            evm_emulator_code_hash,
            l2_system_upgrade_tx_hash,
            l2_system_upgrade_tx_hash_new: l2_system_upgrade_tx_hash,
        }
    }

    pub fn bootloader_code_hash(&self) -> Option<H256> {
        self.bootloader_code_hash
            .or_else(|| self.base_system_contracts.map(|hashes| hashes.bootloader))
    }

    pub fn default_account_code_hash(&self) -> Option<H256> {
        self.default_account_code_hash
            .or_else(|| self.base_system_contracts.map(|hashes| hashes.default_aa))
    }

    pub fn evm_emulator_code_hash(&self) -> Option<H256> {
        self.evm_emulator_code_hash.or_else(|| {
            self.base_system_contracts
                .and_then(|hashes| hashes.evm_emulator)
        })
    }

    pub fn minor_version(&self) -> Option<u16> {
        self.minor_version.or(self.version_id)
    }

    pub fn l2_system_upgrade_tx_hash(&self) -> Option<H256> {
        self.l2_system_upgrade_tx_hash_new
            .or(self.l2_system_upgrade_tx_hash)
    }
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct ProtocolVersionInfo {
    /// Minor version of the protocol
    #[serde(rename = "minorVersion")]
    pub minor_version: u16,
    /// Timestamp at which upgrade should be performed
    pub timestamp: u64,
    /// Bootloader code hash
    #[serde(rename = "bootloaderCodeHash")]
    pub bootloader_code_hash: H256,
    /// Default account code hash
    #[serde(rename = "defaultAccountCodeHash")]
    pub default_account_code_hash: H256,
    /// EVM emulator code hash
    #[serde(rename = "evmEmulatorCodeHash")]
    pub evm_emulator_code_hash: Option<H256>,
    /// L2 Upgrade transaction hash
    #[serde(rename = "l2SystemUpgradeTxHash")]
    pub l2_system_upgrade_tx_hash: Option<H256>,
}

impl TryFrom<ProtocolVersion> for ProtocolVersionInfo {
    type Error = anyhow::Error;

    fn try_from(value: ProtocolVersion) -> Result<Self, Self::Error> {
        Ok(ProtocolVersionInfo {
            minor_version: value
                .minor_version
                .context("missing minor protocol version")?,
            timestamp: value.timestamp,
            bootloader_code_hash: value
                .bootloader_code_hash
                .context("missing bootloader code hash")?,
            default_account_code_hash: value
                .default_account_code_hash
                .context("missing default account code hash")?,
            evm_emulator_code_hash: value.evm_emulator_code_hash,
            l2_system_upgrade_tx_hash: value.l2_system_upgrade_tx_hash_new,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum SupportedTracers {
    CallTracer,
    FlatCallTracer,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, Copy)]
#[serde(rename_all = "camelCase")]
pub struct CallTracerConfig {
    pub only_top_call: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct TracerConfig {
    pub tracer: SupportedTracers,
    #[serde(default)]
    pub tracer_config: CallTracerConfig,
}

impl Default for TracerConfig {
    fn default() -> Self {
        TracerConfig {
            tracer: SupportedTracers::CallTracer,
            tracer_config: CallTracerConfig {
                only_top_call: false,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum BlockStatus {
    Sealed,
    FastFinalized,
    Verified,
}

/// Result tracers need to have a nested result field for compatibility. So we have two different
/// structs 1 for blocks tracing and one for txs and call tracing
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum CallTracerBlockResult {
    CallTrace(Vec<ResultDebugCall>),
    FlatCallTrace(Vec<ResultDebugCallFlat>),
}

impl CallTracerBlockResult {
    pub fn unwrap_flat(self) -> Vec<ResultDebugCallFlat> {
        match self {
            Self::CallTrace(_) => panic!("Result is a FlatCallTrace"),
            Self::FlatCallTrace(trace) => trace,
        }
    }

    pub fn unwrap_default(self) -> Vec<ResultDebugCall> {
        match self {
            Self::CallTrace(trace) => trace,
            Self::FlatCallTrace(_) => panic!("Result is a CallTrace"),
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum CallTracerResult {
    CallTrace(DebugCall),
    FlatCallTrace(Vec<DebugCallFlat>),
}

impl CallTracerResult {
    pub fn unwrap_flat(self) -> Vec<DebugCallFlat> {
        match self {
            Self::CallTrace(_) => panic!("Result is a FlatCallTrace"),
            Self::FlatCallTrace(trace) => trace,
        }
    }

    pub fn unwrap_default(self) -> DebugCall {
        match self {
            Self::CallTrace(trace) => trace,
            Self::FlatCallTrace(_) => panic!("Result is a CallTrace"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockDetailsBase {
    pub timestamp: u64,
    pub l1_tx_count: usize,
    pub l2_tx_count: usize,
    /// Hash for an L2 block, or the root hash (aka state hash) for an L1 batch.
    pub root_hash: Option<H256>,
    pub status: BlockStatus,
    pub commit_tx_hash: Option<H256>,
    pub committed_at: Option<DateTime<Utc>>,
    pub commit_tx_finality: Option<EthTxFinalityStatus>,
    pub commit_chain_id: Option<SLChainId>,
    pub prove_tx_hash: Option<H256>,
    pub prove_tx_finality: Option<EthTxFinalityStatus>,
    pub proven_at: Option<DateTime<Utc>>,
    pub prove_chain_id: Option<SLChainId>,
    pub execute_tx_hash: Option<H256>,
    pub execute_tx_finality: Option<EthTxFinalityStatus>,
    pub executed_at: Option<DateTime<Utc>>,
    pub execute_chain_id: Option<SLChainId>,
    pub l1_gas_price: u64,
    pub l2_fair_gas_price: u64,
    // Cost of publishing one byte (in wei).
    pub fair_pubdata_price: Option<u64>,
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockDetails {
    pub number: L2BlockNumber,
    pub l1_batch_number: L1BatchNumber,
    #[serde(flatten)]
    pub base: BlockDetailsBase,
    pub operator_address: Address,
    pub protocol_version: Option<ProtocolVersionId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct L1BatchDetails {
    pub number: L1BatchNumber,
    #[serde(flatten)]
    pub base: BlockDetailsBase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageProof {
    pub key: H256,
    pub proof: Vec<H256>,
    pub value: H256,
    pub index: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Proof {
    pub address: Address,
    pub storage_proof: Vec<StorageProof>,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeeProof {
    pub l1_batch_number: L1BatchNumber,
    pub tee_type: Option<TeeType>,
    #[serde_as(as = "Option<Hex>")]
    pub pubkey: Option<Vec<u8>>,
    #[serde_as(as = "Option<Hex>")]
    pub signature: Option<Vec<u8>>,
    #[serde_as(as = "Option<Hex>")]
    pub proof: Option<Vec<u8>>,
    pub proved_at: DateTime<Utc>,
    pub status: String,
    #[serde_as(as = "Option<Hex>")]
    pub attestation: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDetailedResult {
    pub transaction_hash: H256,
    pub storage_logs: Vec<ApiStorageLog>,
    pub events: Vec<Log>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiStorageLog {
    pub address: Address,
    pub key: U256,
    pub written_value: U256,
}

/// Raw transaction execution data.
/// Data is taken from `TransactionExecutionMetrics`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionExecutionInfo {
    pub execution_info: Value,
}

/// The fee history type returned from `eth_feeHistory` call.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeHistory {
    #[serde(flatten)]
    pub inner: zksync_basic_types::web3::FeeHistory,
    /// An array of effective pubdata prices. Note, that this field is L2-specific and only provided by L2 nodes.
    #[serde(default)]
    pub l2_pubdata_price: Vec<U256>,
}

/// The data availability details type. Used exclusively in Validiums.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataAvailabilityDetails {
    pub pubdata_type: Option<PubdataType>,
    pub blob_id: String,
    pub inclusion_data: Option<Vec<u8>>,
    pub sent_at: DateTime<Utc>,
    pub l2_da_validator: Option<Address>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct L1ToL2TxsStatus {
    pub l1_to_l2_txs_in_mempool: usize,
    pub l1_to_l2_txs_paused: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayMigrationStatus {
    pub latest_notification: Option<GatewayMigrationNotification>,
    pub state: GatewayMigrationState,
    pub settlement_layer: Option<SettlementLayer>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct EcosystemContracts {
    pub bridgehub_proxy_addr: Address,
    pub state_transition_proxy_addr: Option<Address>,
    pub transparent_proxy_admin_addr: Option<Address>,
    pub l1_bytecodes_supplier_addr: Option<Address>,
    // Note that on the contract side of things this contract is called `L2WrappedBaseTokenStore`,
    // while on the server side for consistency with the conventions, where the prefix denotes
    // the location of the contracts we call it `l1_wrapped_base_token_store`
    pub l1_wrapped_base_token_store: Option<Address>,
    pub server_notifier_addr: Option<Address>,
    pub message_root_proxy_addr: Option<Address>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO (PLA-965): remove test after removing deprecating fields.
    #[allow(deprecated)]
    #[test]
    fn check_protocol_version_type_compatibility() {
        let new_version = ProtocolVersion {
            version_id: Some(24),
            minor_version: Some(24),
            timestamp: 0,
            verification_keys_hashes: Some(Default::default()),
            base_system_contracts: Some(Default::default()),
            bootloader_code_hash: Some(Default::default()),
            default_account_code_hash: Some(Default::default()),
            evm_emulator_code_hash: Some(Default::default()),
            l2_system_upgrade_tx_hash: Default::default(),
            l2_system_upgrade_tx_hash_new: Default::default(),
        };

        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct OldProtocolVersion {
            pub version_id: u16,
            pub timestamp: u64,
            pub verification_keys_hashes: L1VerifierConfig,
            pub base_system_contracts: BaseSystemContractsHashes,
            pub l2_system_upgrade_tx_hash: Option<H256>,
        }

        serde_json::from_str::<OldProtocolVersion>(&serde_json::to_string(&new_version).unwrap())
            .unwrap();
    }

    #[test]
    fn proper_display() {
        let block_number = BlockNumber::Committed;
        assert_eq!(format!("{}", block_number), "Committed");
        let block_number = BlockNumber::Finalized;
        assert_eq!(format!("{}", block_number), "Finalized");
        let block_number = BlockNumber::Latest;
        assert_eq!(format!("{}", block_number), "Latest");
        let block_number = BlockNumber::L1Committed;
        assert_eq!(format!("{}", block_number), "L1Committed");
        let block_number = BlockNumber::FastFinalized;
        assert_eq!(format!("{}", block_number), "FastFinalized");
        let block_number = BlockNumber::Earliest;
        assert_eq!(format!("{}", block_number), "Earliest");
        let block_number = BlockNumber::Pending;
        assert_eq!(format!("{}", block_number), "Pending");
        let block_number = BlockNumber::Number(U64::from(42));
        assert_eq!(format!("{}", block_number), "42");
    }
}

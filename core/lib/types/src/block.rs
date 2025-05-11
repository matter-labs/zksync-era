use std::fmt;

use serde::{
    de::{self, MapAccess, Visitor},
    ser::SerializeStruct,
    Deserialize, Deserializer, Serialize, Serializer,
};
use zksync_basic_types::{commitment::PubdataParams, Address, Bloom, BloomInput, H256, U256};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_system_constants::SYSTEM_BLOCK_INFO_BLOCK_NUMBER_MULTIPLIER;

use crate::{
    fee_model::BatchFeeInput,
    l2_to_l1_log::{SystemL2ToL1Log, UserL2ToL1Log},
    priority_op_onchain_data::PriorityOpOnchainData,
    web3::{keccak256, keccak256_concat},
    AccountTreeId, InteropRoot, L1BatchNumber, L2BlockNumber, ProtocolVersionId, Transaction,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeployedContract {
    pub account_id: AccountTreeId,
    pub bytecode: Vec<u8>,
}

impl DeployedContract {
    pub fn new(account_id: AccountTreeId, bytecode: Vec<u8>) -> Self {
        Self {
            account_id,
            bytecode,
        }
    }
}

/// Holder for l1 batches data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct L1BatchStatistics {
    pub number: L1BatchNumber,
    pub timestamp: u64,
    pub l2_tx_count: u32,
    pub l1_tx_count: u32,
}

impl From<L1BatchHeader> for L1BatchStatistics {
    fn from(header: L1BatchHeader) -> Self {
        Self {
            number: header.number,
            timestamp: header.timestamp,
            l1_tx_count: header.l1_tx_count.into(),
            l2_tx_count: header.l2_tx_count.into(),
        }
    }
}

/// Holder for the block metadata that is not available from transactions themselves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct L1BatchHeader {
    /// Numeric ID of the block. Starts from 1, 0 block is considered genesis block and has no transactions.
    pub number: L1BatchNumber,
    /// Timestamp when block was first created.
    pub timestamp: u64,
    /// Total number of processed priority operations in the block
    pub l1_tx_count: u16,
    /// Total number of processed txs that was requested offchain
    pub l2_tx_count: u16,
    /// The data of the processed priority operations hash which must be sent to the smart contract.
    pub priority_ops_onchain_data: Vec<PriorityOpOnchainData>,
    /// All user generated L2 -> L1 logs in the block.
    pub l2_to_l1_logs: Vec<UserL2ToL1Log>,
    /// Preimages of the hashes that were sent as value of L2 logs by special system L2 contract.
    pub l2_to_l1_messages: Vec<Vec<u8>>,
    /// Bloom filter for the event logs in the block.
    pub bloom: Bloom,
    /// Hashes of contracts used this block
    pub used_contract_hashes: Vec<U256>,
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
    /// System logs are those emitted as part of the Vm execution.
    pub system_logs: Vec<SystemL2ToL1Log>,
    /// Version of protocol used for the L1 batch.
    pub protocol_version: Option<ProtocolVersionId>,
    pub pubdata_input: Option<Vec<u8>>,
    pub fee_address: Address,
    pub batch_fee_input: BatchFeeInput,
}

impl L1BatchHeader {
    pub fn to_unsealed_header(&self) -> UnsealedL1BatchHeader {
        UnsealedL1BatchHeader {
            number: self.number,
            timestamp: self.timestamp,
            protocol_version: self.protocol_version,
            fee_address: self.fee_address,
            fee_input: self.batch_fee_input,
        }
    }
}

/// Holder for the metadata that is relevant for unsealed batches.
#[derive(Debug, Clone, PartialEq)]
pub struct UnsealedL1BatchHeader {
    pub number: L1BatchNumber,
    pub timestamp: u64,
    pub protocol_version: Option<ProtocolVersionId>,
    pub fee_address: Address,
    pub fee_input: BatchFeeInput,
}

/// Holder for the metadata that is relevant for both sealed and unsealed batches.
pub struct CommonL1BatchHeader {
    pub number: L1BatchNumber,
    pub is_sealed: bool,
    pub timestamp: u64,
    pub protocol_version: Option<ProtocolVersionId>,
    pub fee_address: Address,
    pub fee_input: BatchFeeInput,
}

/// Holder for the L2 block metadata that is not available from transactions themselves.
#[derive(Debug, Clone, PartialEq)]
pub struct L2BlockHeader {
    pub number: L2BlockNumber,
    pub timestamp: u64,
    pub hash: H256,
    pub l1_tx_count: u16,
    pub l2_tx_count: u16,
    pub fee_account_address: Address,
    pub base_fee_per_gas: u64, // Min wei per gas that txs in this L2 block need to have.

    pub batch_fee_input: BatchFeeInput,
    pub gas_per_pubdata_limit: u64,
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
    pub protocol_version: Option<ProtocolVersionId>,
    /// The maximal number of virtual blocks to be created in the L2 block.
    pub virtual_blocks: u32,

    /// The formal value of the gas limit for the L2 block.
    /// This value should bound the maximal amount of gas that can be spent by transactions in the L2 block.
    /// Note, that it is an `u64`, i.e. while the computational limit for the bootloader is an `u32` a much larger
    /// amount of gas can be spent on pubdata.
    pub gas_limit: u64,
    pub logs_bloom: Bloom,
    pub pubdata_params: PubdataParams,
}

/// Structure that represents the data is returned by the storage oracle during batch execution.
pub struct StorageOracleInfo {
    /// The refunds returned by the storage oracle.
    pub storage_refunds: Vec<u32>,
    // Pubdata costs are available only since v1.5.0, so we allow them to be optional.
    pub pubdata_costs: Option<Vec<i32>>,
}

/// Data needed to execute an L2 block in the VM.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct L2BlockExecutionData {
    pub number: L2BlockNumber,
    pub timestamp: u64,
    pub prev_block_hash: H256,
    pub virtual_blocks: u32,
    pub txs: Vec<Transaction>,
    pub interop_roots: Vec<InteropRoot>,
}

impl L1BatchHeader {
    pub fn new(
        number: L1BatchNumber,
        timestamp: u64,
        base_system_contracts_hashes: BaseSystemContractsHashes,
        protocol_version: ProtocolVersionId,
    ) -> L1BatchHeader {
        Self {
            number,
            timestamp,
            l1_tx_count: 0,
            l2_tx_count: 0,
            priority_ops_onchain_data: vec![],
            l2_to_l1_logs: vec![],
            l2_to_l1_messages: vec![],
            bloom: Bloom::default(),
            used_contract_hashes: vec![],
            base_system_contracts_hashes,
            system_logs: vec![],
            protocol_version: Some(protocol_version),
            pubdata_input: Some(vec![]),
            fee_address: Default::default(),
            batch_fee_input: BatchFeeInput::pubdata_independent(0, 0, 0),
        }
    }

    /// Creates a hash of the priority ops data.
    pub fn priority_ops_onchain_data_hash(&self) -> H256 {
        let mut rolling_hash: H256 = keccak256(&[]).into();
        for onchain_data in &self.priority_ops_onchain_data {
            let mut preimage = Vec::new();
            preimage.extend(rolling_hash.as_bytes());
            preimage.extend(onchain_data.onchain_data_hash.as_bytes());

            rolling_hash = keccak256(&preimage).into();
        }

        rolling_hash
    }

    pub fn tx_count(&self) -> usize {
        (self.l1_tx_count + self.l2_tx_count) as usize
    }
}

/// Hasher of L2 block contents used by the VM.
#[derive(Debug)]
pub struct L2BlockHasher {
    number: L2BlockNumber,
    timestamp: u64,
    prev_l2_block_hash: H256,
    txs_rolling_hash: H256,
}

impl L2BlockHasher {
    /// At the beginning of the ZKsync, the hashes of the blocks could be calculated as the hash of their number.
    /// This method returns the hash of such L2 blocks.
    pub fn legacy_hash(l2_block_number: L2BlockNumber) -> H256 {
        H256(keccak256(&l2_block_number.0.to_be_bytes()))
    }

    /// Creates a new hasher with the specified params. This assumes a L2 block without transactions;
    /// transaction hashes can be supplied using [`Self::push_tx_hash()`].
    pub fn new(number: L2BlockNumber, timestamp: u64, prev_l2_block_hash: H256) -> Self {
        Self {
            number,
            timestamp,
            prev_l2_block_hash,
            txs_rolling_hash: H256::zero(),
        }
    }

    /// Updates this hasher with a transaction hash. This should be called for all transactions in the block
    /// in the order of their execution.
    pub fn push_tx_hash(&mut self, tx_hash: H256) {
        self.txs_rolling_hash = keccak256_concat(self.txs_rolling_hash, tx_hash);
    }

    /// Returns the hash of the L2 block.
    ///
    /// For newer protocol versions, the hash is computed as
    ///
    /// ```text
    /// keccak256(u256_be(number) ++ u256_be(timestamp) ++ prev_l2_block_hash ++ txs_rolling_hash)
    /// ```
    ///
    /// Here, `u256_be` is the big-endian 256-bit serialization of a number, and `txs_rolling_hash`
    /// is *the rolling hash* of L2 block transactions. `txs_rolling_hash` is calculated the following way:
    ///
    /// - If the L2 block has 0 transactions, then `txs_rolling_hash` is equal to `H256::zero()`.
    /// - If the L2 block has i transactions, then `txs_rolling_hash` is equal to `H(H_{i-1}, H(tx_i))`, where
    ///   `H_{i-1}` is the `txs_rolling_hash` of the first `i - 1` transactions.
    pub fn finalize(self, protocol_version: ProtocolVersionId) -> H256 {
        if protocol_version >= ProtocolVersionId::Version13 {
            let mut digest = [0_u8; 128];
            U256::from(self.number.0).to_big_endian(&mut digest[0..32]);
            U256::from(self.timestamp).to_big_endian(&mut digest[32..64]);
            digest[64..96].copy_from_slice(self.prev_l2_block_hash.as_bytes());
            digest[96..128].copy_from_slice(self.txs_rolling_hash.as_bytes());
            H256(keccak256(&digest))
        } else {
            Self::legacy_hash(self.number)
        }
    }

    pub fn hash(
        number: L2BlockNumber,
        timestamp: u64,
        prev_l2_block_hash: H256,
        txs_rolling_hash: H256,
        protocol_version: ProtocolVersionId,
    ) -> H256 {
        Self {
            number,
            timestamp,
            prev_l2_block_hash,
            txs_rolling_hash,
        }
        .finalize(protocol_version)
    }
}

/// Returns block.number/timestamp based on the block's information
pub fn unpack_block_info(info: U256) -> (u64, u64) {
    let block_number = (info / SYSTEM_BLOCK_INFO_BLOCK_NUMBER_MULTIPLIER).as_u64();
    let block_timestamp = (info % SYSTEM_BLOCK_INFO_BLOCK_NUMBER_MULTIPLIER).as_u64();
    (block_number, block_timestamp)
}

/// Transforms block number and timestamp into a packed 32-byte representation
pub fn pack_block_info(block_number: u64, block_timestamp: u64) -> U256 {
    U256::from(block_number) * SYSTEM_BLOCK_INFO_BLOCK_NUMBER_MULTIPLIER
        + U256::from(block_timestamp)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct L1BatchTreeData {
    pub hash: H256,
    pub rollup_last_leaf_index: u64,
}

pub fn build_bloom<'a, I: IntoIterator<Item = BloomInput<'a>>>(items: I) -> Bloom {
    let mut bloom = Bloom::zero();
    for item in items {
        bloom.accrue(item);
    }

    bloom
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BatchOrBlockNumber {
    BatchNumber(L1BatchNumber),
    BlockNumber(L2BlockNumber),
}

impl Serialize for BatchOrBlockNumber {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            BatchOrBlockNumber::BatchNumber(val) => val.serialize(serializer),
            BatchOrBlockNumber::BlockNumber(val) => {
                let mut state = serializer.serialize_struct("BlockObject", 1)?;
                state.serialize_field("block", val)?;
                state.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for BatchOrBlockNumber {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BatchOrBlockNumberVisitor;

        impl<'de> Visitor<'de> for BatchOrBlockNumberVisitor {
            type Value = BatchOrBlockNumber;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a JSON number (for L1BatchNumber) or a JSON object like {\"block\": number} (for L2BlockNumber)")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let num = u32::try_from(value).map_err(|_| {
                    E::invalid_value(de::Unexpected::Unsigned(value), &"a u32 number")
                })?;
                Ok(BatchOrBlockNumber::BatchNumber(L1BatchNumber(num)))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if value < 0 {
                    return Err(E::invalid_value(
                        de::Unexpected::Signed(value),
                        &"a non-negative u32 number",
                    ));
                }
                let num = u32::try_from(value).map_err(|_| {
                    E::invalid_value(de::Unexpected::Signed(value), &"a u32 number")
                })?;
                Ok(BatchOrBlockNumber::BatchNumber(L1BatchNumber(num)))
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                #[derive(Deserialize)]
                struct BlockWrapper {
                    block: L2BlockNumber,
                }

                let wrapper: BlockWrapper =
                    Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(BatchOrBlockNumber::BlockNumber(wrapper.block))
            }
        }

        deserializer.deserialize_any(BatchOrBlockNumberVisitor)
    }
}

#[cfg(test)]
mod tests {
    use std::{iter, str::FromStr};

    use super::*;

    #[test]
    fn test_legacy_l2_block_hashes() {
        // The comparing with the hash taken from explorer
        let expected_hash = "6a13b75b5982035ebb28999fbf6f54e7d7fad9e290d5c5f99e7c7d75d42b6099"
            .parse()
            .unwrap();
        assert_eq!(
            L2BlockHasher::legacy_hash(L2BlockNumber(11470850)),
            expected_hash
        )
    }

    #[test]
    fn test_l2_block_hash() {
        // Comparing with a constant hash generated from a contract:
        let expected_hash: H256 =
            "c4e184fa9dde8d81aa085f3d1831b00be0a2f4e40218ff1b3456684e7eeccdfe"
                .parse()
                .unwrap();
        let prev_l2_block_hash = "9b14f83c434b860168ed4081f7b2a65f432f68bfea86ddf3351c02bc855dd721"
            .parse()
            .unwrap();
        let txs_rolling_hash = "67506e289f13aee79b8de3bfd99f460f46135028b85eee9da760a17a4453fb64"
            .parse()
            .unwrap();
        assert_eq!(
            expected_hash,
            L2BlockHasher {
                number: L2BlockNumber(1),
                timestamp: 12,
                prev_l2_block_hash,
                txs_rolling_hash,
            }
            .finalize(ProtocolVersionId::latest())
        )
    }

    #[test]
    fn test_block_packing() {
        let block_number = 101;
        let block_timestamp = 102;
        let block_info = pack_block_info(block_number, block_timestamp);

        let (unpacked_block_number, unpacked_block_timestamp) = unpack_block_info(block_info);
        assert_eq!(block_number, unpacked_block_number);
        assert_eq!(block_timestamp, unpacked_block_timestamp);
    }

    #[test]
    fn test_build_bloom() {
        let logs = [
            (
                Address::from_str("0x86Fa049857E0209aa7D9e616F7eb3b3B78ECfdb0").unwrap(),
                vec![
                    H256::from_str(
                        "0x3452f51d00000000000000000000000000000000000000000000000000000000",
                    )
                    .unwrap(),
                    H256::from_str(
                        "0x000000000000000000000000d0a6e6c54dbc68db5db3a091b171a77407ff7ccf",
                    )
                    .unwrap(),
                    H256::from_str(
                        "0x0000000000000000000000000f5e378a82a55f24e88317a8fb7cd2ed8bd3873f",
                    )
                    .unwrap(),
                    H256::from_str(
                        "0x000000000000000000000000000000000000000000000004f0e6ade1e67bb719",
                    )
                    .unwrap(),
                ],
            ),
            (
                Address::from_str("0x86Fa049857E0209aa7D9e616F7eb3b3B78ECfdb0").unwrap(),
                vec![
                    H256::from_str(
                        "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
                    )
                    .unwrap(),
                    H256::from_str(
                        "0x000000000000000000000000d0a6e6c54dbc68db5db3a091b171a77407ff7ccf",
                    )
                    .unwrap(),
                    H256::from_str(
                        "0x0000000000000000000000000f5e378a82a55f24e88317a8fb7cd2ed8bd3873f",
                    )
                    .unwrap(),
                ],
            ),
            (
                Address::from_str("0xd0a6E6C54DbC68Db5db3A091B171A77407Ff7ccf").unwrap(),
                vec![H256::from_str(
                    "0x51223fdc0a25891366fb358b4af9fe3c381b1566e287c61a29d01c8a173fe4f4",
                )
                .unwrap()],
            ),
        ];
        let iter = logs.iter().flat_map(|log| {
            log.1
                .iter()
                .map(|topic| BloomInput::Raw(topic.as_bytes()))
                .chain(iter::once(BloomInput::Raw(log.0.as_bytes())))
        });

        let bloom = build_bloom(iter);
        let expected = Bloom::from_str(
            "0000000004000000000000000100000000000000000000000000000000000000\
            0000000000000000000040000000000000000000000000000000000000000200\
            0000000000020000400000180000000000000000000000000000000000000000\
            0000000000000000000000000000000000000000080000000000201000000000\
            2000000000000000400000000000080000008000000000000000000000000000\
            0000000000000000000000000004000000000001000000000000804000000000\
            0000000200000000000000000000000400000000000000000000000800200000\
            0000000000000010000000000000000000000000000000000000000000000000",
        )
        .unwrap();

        assert_eq!(bloom, expected);
    }
}

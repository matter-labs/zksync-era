use std::{fmt, ops};

use serde::{Deserialize, Serialize};
use zksync_basic_types::{H2048, H256, U256};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_system_constants::SYSTEM_BLOCK_INFO_BLOCK_NUMBER_MULTIPLIER;
use zksync_utils::concat_and_hash;

use crate::{
    fee_model::BatchFeeInput,
    l2_to_l1_log::{SystemL2ToL1Log, UserL2ToL1Log},
    priority_op_onchain_data::PriorityOpOnchainData,
    web3::signing::keccak256,
    AccountTreeId, Address, L1BatchNumber, MiniblockNumber, ProtocolVersionId, Transaction,
};

/// Represents a successfully deployed smart contract.
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

/// Holder for the block metadata that is not available from transactions themselves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct L1BatchHeader {
    /// Numeric ID of the block. Starts from 1, 0 block is considered genesis block and has no transactions.
    pub number: L1BatchNumber,
    /// Whether block is sealed or not (doesn't correspond to committing/verifying it on the L1).
    pub is_finished: bool,
    /// Timestamp when block was first created.
    pub timestamp: u64,
    /// Address of the fee account that was used when block was created
    pub fee_account_address: Address,
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
    pub bloom: H2048,
    /// Hashes of contracts used this block
    pub used_contract_hashes: Vec<U256>,
    /// The EIP1559 base_fee used in this block.
    pub base_fee_per_gas: u64,
    /// The assumed L1 gas price within the block.
    pub l1_gas_price: u64,
    /// The L2 gas price that the operator agrees on.
    pub l2_fair_gas_price: u64,
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
    /// System logs are those emitted as part of the Vm execution.
    pub system_logs: Vec<SystemL2ToL1Log>,
    /// Version of protocol used for the L1 batch.
    pub protocol_version: Option<ProtocolVersionId>,
    pub pubdata_input: Option<Vec<u8>>,
}

/// Holder for the miniblock metadata that is not available from transactions themselves.
#[derive(Debug, Clone, PartialEq)]
pub struct MiniblockHeader {
    pub number: MiniblockNumber,
    pub timestamp: u64,
    pub hash: H256,
    pub l1_tx_count: u16,
    pub l2_tx_count: u16,
    pub base_fee_per_gas: u64, // Min wei per gas that txs in this miniblock need to have.

    pub batch_fee_input: BatchFeeInput,
    pub gas_per_pubdata_limit: u64,
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
    pub protocol_version: Option<ProtocolVersionId>,
    /// The maximal number of virtual blocks to be created in the miniblock.
    pub virtual_blocks: u32,
}

/// Data needed to execute a miniblock in the VM.
#[derive(Debug)]
pub struct MiniblockExecutionData {
    pub number: MiniblockNumber,
    pub timestamp: u64,
    pub prev_block_hash: H256,
    pub virtual_blocks: u32,
    pub txs: Vec<Transaction>,
}

impl L1BatchHeader {
    pub fn new(
        number: L1BatchNumber,
        timestamp: u64,
        fee_account_address: Address,
        base_system_contracts_hashes: BaseSystemContractsHashes,
        protocol_version: ProtocolVersionId,
    ) -> L1BatchHeader {
        Self {
            number,
            is_finished: false,
            timestamp,
            fee_account_address,
            l1_tx_count: 0,
            l2_tx_count: 0,
            priority_ops_onchain_data: vec![],
            l2_to_l1_logs: vec![],
            l2_to_l1_messages: vec![],
            bloom: H2048::default(),
            used_contract_hashes: vec![],
            base_fee_per_gas: 0,
            l1_gas_price: 0,
            l2_fair_gas_price: 0,
            base_system_contracts_hashes,
            system_logs: vec![],
            protocol_version: Some(protocol_version),
            pubdata_input: Some(vec![]),
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

#[derive(Clone, Copy, Eq, PartialEq, Default)]
pub struct BlockGasCount {
    pub commit: u32,
    pub prove: u32,
    pub execute: u32,
}

impl fmt::Debug for BlockGasCount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "c:{}/p:{}/e:{}",
            self.commit, self.prove, self.execute
        )
    }
}

impl BlockGasCount {
    pub fn any_field_greater_than(&self, bound: u32) -> bool {
        self.commit > bound || self.prove > bound || self.execute > bound
    }
}

impl ops::Add for BlockGasCount {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            commit: self.commit + rhs.commit,
            prove: self.prove + rhs.prove,
            execute: self.execute + rhs.execute,
        }
    }
}

impl ops::AddAssign for BlockGasCount {
    fn add_assign(&mut self, other: Self) {
        *self = Self {
            commit: self.commit + other.commit,
            prove: self.prove + other.prove,
            execute: self.execute + other.execute,
        };
    }
}

/// Hasher of miniblock contents used by the VM.
#[derive(Debug)]
pub struct MiniblockHasher {
    number: MiniblockNumber,
    timestamp: u64,
    prev_miniblock_hash: H256,
    txs_rolling_hash: H256,
}

impl MiniblockHasher {
    /// At the beginning of the zkSync, the hashes of the blocks could be calculated as the hash of their number.
    /// This method returns the hash of such miniblocks.
    pub fn legacy_hash(miniblock_number: MiniblockNumber) -> H256 {
        H256(keccak256(&miniblock_number.0.to_be_bytes()))
    }

    /// Creates a new hasher with the specified params. This assumes a miniblock without transactions;
    /// transaction hashes can be supplied using [`Self::push_tx_hash()`].
    pub fn new(number: MiniblockNumber, timestamp: u64, prev_miniblock_hash: H256) -> Self {
        Self {
            number,
            timestamp,
            prev_miniblock_hash,
            txs_rolling_hash: H256::zero(),
        }
    }

    /// Updates this hasher with a transaction hash. This should be called for all transactions in the block
    /// in the order of their execution.
    pub fn push_tx_hash(&mut self, tx_hash: H256) {
        self.txs_rolling_hash = concat_and_hash(self.txs_rolling_hash, tx_hash);
    }

    /// Returns the hash of the miniblock.
    ///
    /// For newer protocol versions, the hash is computed as
    ///
    /// ```text
    /// keccak256(u256_be(number) ++ u256_be(timestamp) ++ prev_miniblock_hash ++ txs_rolling_hash)
    /// ```
    ///
    /// Here, `u256_be` is the big-endian 256-bit serialization of a number, and `txs_rolling_hash`
    /// is *the rolling hash* of miniblock transactions. `txs_rolling_hash` is calculated the following way:
    ///
    /// - If the miniblock has 0 transactions, then `txs_rolling_hash` is equal to `H256::zero()`.
    /// - If the miniblock has i transactions, then `txs_rolling_hash` is equal to `H(H_{i-1}, H(tx_i))`, where
    ///   `H_{i-1}` is the `txs_rolling_hash` of the first i-1 transactions.
    pub fn finalize(self, protocol_version: ProtocolVersionId) -> H256 {
        if protocol_version >= ProtocolVersionId::Version13 {
            let mut digest = [0_u8; 128];
            U256::from(self.number.0).to_big_endian(&mut digest[0..32]);
            U256::from(self.timestamp).to_big_endian(&mut digest[32..64]);
            digest[64..96].copy_from_slice(self.prev_miniblock_hash.as_bytes());
            digest[96..128].copy_from_slice(self.txs_rolling_hash.as_bytes());
            H256(keccak256(&digest))
        } else {
            Self::legacy_hash(self.number)
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legacy_miniblock_hashes() {
        // The comparing with the hash taken from explorer
        let expected_hash = "6a13b75b5982035ebb28999fbf6f54e7d7fad9e290d5c5f99e7c7d75d42b6099"
            .parse()
            .unwrap();
        assert_eq!(
            MiniblockHasher::legacy_hash(MiniblockNumber(11470850)),
            expected_hash
        )
    }

    #[test]
    fn test_miniblock_hash() {
        // Comparing with a constant hash generated from a contract:
        let expected_hash: H256 =
            "c4e184fa9dde8d81aa085f3d1831b00be0a2f4e40218ff1b3456684e7eeccdfe"
                .parse()
                .unwrap();
        let prev_miniblock_hash =
            "9b14f83c434b860168ed4081f7b2a65f432f68bfea86ddf3351c02bc855dd721"
                .parse()
                .unwrap();
        let txs_rolling_hash = "67506e289f13aee79b8de3bfd99f460f46135028b85eee9da760a17a4453fb64"
            .parse()
            .unwrap();
        assert_eq!(
            expected_hash,
            MiniblockHasher {
                number: MiniblockNumber(1),
                timestamp: 12,
                prev_miniblock_hash,
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
}

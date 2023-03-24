use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};
use std::ops::{Add, AddAssign};
use zksync_basic_types::{H2048, H256, U256};
use zksync_contracts::BaseSystemContractsHashes;

use crate::{
    l2_to_l1_log::L2ToL1Log, priority_op_onchain_data::PriorityOpOnchainData,
    pubdata_packing::pack_storage_log, web3::signing::keccak256, AccountTreeId, Address,
    L1BatchNumber, MiniblockNumber, StorageKey, StorageLogKind, WitnessStorageLog,
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
    /// The data of the processed priority operations hash which must be sent to the smart contract
    pub priority_ops_onchain_data: Vec<PriorityOpOnchainData>,
    /// all L2 -> L1 logs in the block
    pub l2_to_l1_logs: Vec<L2ToL1Log>,
    /// preimages of the hashes that were sent as value of L2 logs by special system L2 contract
    pub l2_to_l1_messages: Vec<Vec<u8>>,
    /// Bloom filter for the event logs in the block.
    pub bloom: H2048,
    /// Initial value of the bootloader's heap
    pub initial_bootloader_contents: Vec<(usize, U256)>,
    /// Hashes of contracts used this block
    pub used_contract_hashes: Vec<U256>,
    /// The EIP1559 base_fee used in this block.
    pub base_fee_per_gas: u64,
    /// The assumed L1 gas price within the block.
    pub l1_gas_price: u64,
    /// The L2 gas price that the operator agrees on.
    pub l2_fair_gas_price: u64,
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
}

/// Holder for the miniblock metadata that is not available from transactions themselves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MiniblockHeader {
    pub number: MiniblockNumber,
    pub timestamp: u64,
    pub hash: H256,
    pub l1_tx_count: u16,
    pub l2_tx_count: u16,
    pub base_fee_per_gas: u64, // Min wei per gas that txs in this miniblock need to have.

    pub l1_gas_price: u64, // L1 gas price assumed in the corresponding batch
    pub l2_fair_gas_price: u64, // L2 gas price assumed in the corresponding batch
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
}

impl L1BatchHeader {
    pub fn new(
        number: L1BatchNumber,
        timestamp: u64,
        fee_account_address: Address,
        base_system_contracts_hashes: BaseSystemContractsHashes,
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
            initial_bootloader_contents: vec![],
            used_contract_hashes: vec![],
            base_fee_per_gas: 0,
            l1_gas_price: 0,
            l2_fair_gas_price: 0,
            base_system_contracts_hashes,
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

/// Utility structure that holds the block header together with its logs required to generate the witness
#[derive(Debug)]
pub struct WitnessBlockWithLogs {
    pub header: L1BatchHeader,
    pub storage_logs: Vec<WitnessStorageLog>,
}

impl WitnessBlockWithLogs {
    /// Packs the logs into the byte sequence.
    /// Used for the onchain data availability.
    pub fn compress_logs<F>(&self, hash_fn: F) -> Vec<u8>
    where
        F: Fn(&StorageKey) -> Vec<u8> + Copy,
    {
        self.storage_logs
            .iter()
            .filter(|log| log.storage_log.kind == StorageLogKind::Write)
            .flat_map(|l| pack_storage_log(&l.storage_log, hash_fn))
            .collect()
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Default)]
pub struct BlockGasCount {
    pub commit: u32,
    pub prove: u32,
    pub execute: u32,
}

impl Debug for BlockGasCount {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "c:{}/p:{}/e:{}", self.commit, self.prove, self.execute)?;
        Ok(())
    }
}

impl BlockGasCount {
    pub fn has_greater_than(&self, bound: u32) -> bool {
        self.commit > bound || self.prove > bound || self.execute > bound
    }
}

impl AddAssign for BlockGasCount {
    fn add_assign(&mut self, other: Self) {
        *self = Self {
            commit: self.commit + other.commit,
            prove: self.prove + other.prove,
            execute: self.execute + other.execute,
        };
    }
}

impl Add for BlockGasCount {
    type Output = BlockGasCount;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            commit: self.commit + rhs.commit,
            prove: self.prove + rhs.prove,
            execute: self.execute + rhs.execute,
        }
    }
}

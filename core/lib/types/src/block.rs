use serde::{Deserialize, Serialize};

use std::{fmt, ops};

use zksync_basic_types::{H2048, H256, U256};
use zksync_contracts::BaseSystemContractsHashes;

use crate::{
    l2_to_l1_log::L2ToL1Log, priority_op_onchain_data::PriorityOpOnchainData,
    web3::signing::keccak256, AccountTreeId, Address, L1BatchNumber, MiniblockNumber,
    ProtocolVersionId, Transaction,
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
    /// All L2 -> L1 logs in the block.
    pub l2_to_l1_logs: Vec<L2ToL1Log>,
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
    /// Version of protocol used for the L1 batch.
    pub protocol_version: Option<ProtocolVersionId>,
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
    pub protocol_version: Option<ProtocolVersionId>,
}

/// Data needed to re-execute miniblock.
#[derive(Debug)]
pub struct MiniblockReexecuteData {
    pub number: MiniblockNumber,
    pub timestamp: u64,
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
            protocol_version: Some(protocol_version),
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

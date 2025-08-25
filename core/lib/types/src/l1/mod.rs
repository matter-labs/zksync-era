//! Definition of ZKsync network priority operations: operations initiated from the L1.

use serde::{Deserialize, Serialize};
use zksync_basic_types::{web3::Log, Address, L1BlockNumber, PriorityOpId, H256, U256};
use zksync_crypto_primitives::hasher::{keccak::KeccakHasher, Hasher};
use zksync_mini_merkle_tree::HashEmptySubtree;

use super::Transaction;
use crate::{
    abi, address_to_u256,
    bytecode::BytecodeHash,
    ethabi,
    helpers::unix_timestamp_ms,
    l1::error::L1TxParseError,
    l2::TransactionType,
    priority_op_onchain_data::{PriorityOpOnchainData, PriorityOpOnchainMetadata},
    tx::Execute,
    u256_to_address, ExecuteTransactionCommon, PRIORITY_OPERATION_L2_TX_TYPE,
    PROTOCOL_UPGRADE_TX_TYPE,
};

pub mod error;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone, Copy)]
#[repr(u8)]
pub enum OpProcessingType {
    Common = 0,
    OnlyRollup = 1,
}

impl TryFrom<u8> for OpProcessingType {
    type Error = ();

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            x if x == OpProcessingType::Common as u8 => Ok(OpProcessingType::Common),
            x if x == OpProcessingType::OnlyRollup as u8 => Ok(OpProcessingType::OnlyRollup),
            _ => Err(()),
        }
    }
}

impl Default for OpProcessingType {
    fn default() -> Self {
        Self::Common
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone, Copy, Default)]
#[repr(u8)]
pub enum PriorityQueueType {
    #[default]
    Deque = 0,
    HeapBuffer = 1,
    Heap = 2,
}

impl TryFrom<u8> for PriorityQueueType {
    type Error = ();

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            x if x == PriorityQueueType::Deque as u8 => Ok(PriorityQueueType::Deque),
            x if x == PriorityQueueType::HeapBuffer as u8 => Ok(PriorityQueueType::HeapBuffer),
            x if x == PriorityQueueType::Heap as u8 => Ok(PriorityQueueType::Heap),
            _ => Err(()),
        }
    }
}

pub fn is_l1_tx_type(tx_type: u8) -> bool {
    tx_type == PRIORITY_OPERATION_L2_TX_TYPE || tx_type == PROTOCOL_UPGRADE_TX_TYPE
}

// TODO(PLA-962): remove once all nodes start treating the deprecated fields as optional.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct L1TxCommonDataSerde {
    pub sender: Address,
    pub serial_id: PriorityOpId,
    pub layer_2_tip_fee: U256,
    pub full_fee: U256,
    pub max_fee_per_gas: U256,
    pub gas_limit: U256,
    pub gas_per_pubdata_limit: U256,
    pub op_processing_type: OpProcessingType,
    pub priority_queue_type: PriorityQueueType,
    pub canonical_tx_hash: H256,
    pub to_mint: U256,
    pub refund_recipient: Address,

    /// DEPRECATED.
    #[serde(default)]
    pub deadline_block: u64,
    #[serde(default)]
    pub eth_hash: H256,
    #[serde(default)]
    pub eth_block: u64,
}

#[derive(Default, Debug, Clone, PartialEq)]
pub struct L1TxCommonData {
    /// Sender of the transaction.
    pub sender: Address,
    /// Unique ID of the priority operation.
    pub serial_id: PriorityOpId,

    /// Additional payment to the operator as an incentive to perform the operation. The contract uses a value of 192 bits.
    pub layer_2_tip_fee: U256,
    /// The total cost the sender paid for the transaction.
    pub full_fee: U256,
    /// The maximal fee per gas to be used for L1->L2 transaction
    pub max_fee_per_gas: U256,
    /// The maximum number of gas that a transaction can spend at a price of gas equals 1.
    pub gas_limit: U256,
    /// The maximum number of gas per 1 byte of pubdata.
    pub gas_per_pubdata_limit: U256,
    /// Indicator that the operation can interact with Rollup and Porter trees, or only with Rollup.
    pub op_processing_type: OpProcessingType,
    /// Priority operations queue type.
    pub priority_queue_type: PriorityQueueType,
    /// Tx hash of the transaction in the ZKsync network. Calculated as the encoded transaction data hash.
    pub canonical_tx_hash: H256,
    /// The amount of ETH that should be minted with this transaction
    pub to_mint: U256,
    /// The recipient of the refund of the transaction
    pub refund_recipient: Address,

    pub eth_block: u64,
}

impl serde::Serialize for L1TxCommonData {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        L1TxCommonDataSerde {
            sender: self.sender,
            serial_id: self.serial_id,
            layer_2_tip_fee: self.layer_2_tip_fee,
            full_fee: self.full_fee,
            max_fee_per_gas: self.max_fee_per_gas,
            gas_limit: self.gas_limit,
            gas_per_pubdata_limit: self.gas_per_pubdata_limit,
            op_processing_type: self.op_processing_type,
            priority_queue_type: self.priority_queue_type,
            canonical_tx_hash: self.canonical_tx_hash,
            to_mint: self.to_mint,
            refund_recipient: self.refund_recipient,

            // DEPRECATED.
            deadline_block: 0,
            eth_hash: H256::default(),
            eth_block: self.eth_block,
        }
        .serialize(s)
    }
}

impl<'de> serde::Deserialize<'de> for L1TxCommonData {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let x = L1TxCommonDataSerde::deserialize(d)?;
        Ok(Self {
            sender: x.sender,
            serial_id: x.serial_id,
            layer_2_tip_fee: x.layer_2_tip_fee,
            full_fee: x.full_fee,
            max_fee_per_gas: x.max_fee_per_gas,
            gas_limit: x.gas_limit,
            gas_per_pubdata_limit: x.gas_per_pubdata_limit,
            op_processing_type: x.op_processing_type,
            priority_queue_type: x.priority_queue_type,
            canonical_tx_hash: x.canonical_tx_hash,
            to_mint: x.to_mint,
            refund_recipient: x.refund_recipient,

            // DEPRECATED.
            eth_block: x.eth_block,
        })
    }
}

impl L1TxCommonData {
    pub fn hash(&self) -> H256 {
        self.canonical_tx_hash
    }

    pub fn onchain_data(&self) -> PriorityOpOnchainData {
        PriorityOpOnchainData {
            layer_2_tip_fee: self.layer_2_tip_fee,
            onchain_data_hash: self.hash(),
        }
    }

    pub fn onchain_metadata(&self) -> PriorityOpOnchainMetadata {
        PriorityOpOnchainMetadata {
            op_processing_type: self.op_processing_type,
            priority_queue_type: self.priority_queue_type,
            onchain_data: self.onchain_data(),
        }
    }

    pub fn tx_format(&self) -> TransactionType {
        TransactionType::PriorityOpTransaction
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L1Tx {
    pub execute: Execute,
    pub common_data: L1TxCommonData,
    pub received_timestamp_ms: u64,
}

impl PartialEq for L1Tx {
    fn eq(&self, other: &Self) -> bool {
        self.execute == other.execute && self.common_data == other.common_data
    }
}

impl HashEmptySubtree<L1Tx> for KeccakHasher {
    fn empty_leaf_hash(&self) -> H256 {
        self.hash_bytes(&[])
    }
}

impl From<L1Tx> for Transaction {
    fn from(tx: L1Tx) -> Self {
        let L1Tx {
            execute,
            common_data,
            received_timestamp_ms,
        } = tx;
        Self {
            common_data: ExecuteTransactionCommon::L1(common_data),
            execute,
            received_timestamp_ms,
            raw_bytes: None,
        }
    }
}

impl TryFrom<Transaction> for L1Tx {
    type Error = &'static str;

    fn try_from(value: Transaction) -> Result<Self, Self::Error> {
        let Transaction {
            common_data,
            execute,
            received_timestamp_ms,
            ..
        } = value;
        match common_data {
            ExecuteTransactionCommon::L1(common_data) => Ok(L1Tx {
                execute,
                common_data,
                received_timestamp_ms,
            }),
            ExecuteTransactionCommon::L2(_) => Err("Cannot convert L2Tx to L1Tx"),
            ExecuteTransactionCommon::ProtocolUpgrade(_) => {
                Err("Cannot convert ProtocolUpgradeTx to L1Tx")
            }
        }
    }
}

impl L1Tx {
    pub fn serial_id(&self) -> PriorityOpId {
        self.common_data.serial_id
    }

    pub fn eth_block(&self) -> L1BlockNumber {
        L1BlockNumber(self.common_data.eth_block as u32)
    }

    pub fn hash(&self) -> H256 {
        self.common_data.hash()
    }
}

impl From<L1Tx> for abi::NewPriorityRequest {
    fn from(t: L1Tx) -> Self {
        let factory_deps = t.execute.factory_deps;
        Self {
            tx_id: t.common_data.serial_id.0.into(),
            tx_hash: t.common_data.canonical_tx_hash.to_fixed_bytes(),
            expiration_timestamp: 0,
            transaction: abi::L2CanonicalTransaction {
                tx_type: PRIORITY_OPERATION_L2_TX_TYPE.into(),
                from: address_to_u256(&t.common_data.sender),
                // Unwrap used here because the contract address should always be present for L1 transactions.
                // TODO: Consider restricting the contract address to not be optional in L1Tx.
                to: address_to_u256(&t.execute.contract_address.unwrap()),
                gas_limit: t.common_data.gas_limit,
                gas_per_pubdata_byte_limit: t.common_data.gas_per_pubdata_limit,
                max_fee_per_gas: t.common_data.max_fee_per_gas,
                max_priority_fee_per_gas: 0.into(),
                paymaster: 0.into(),
                nonce: t.common_data.serial_id.0.into(),
                value: t.execute.value,
                reserved: [
                    t.common_data.to_mint,
                    address_to_u256(&t.common_data.refund_recipient),
                    0.into(),
                    0.into(),
                ],
                data: t.execute.calldata,
                signature: vec![],
                factory_deps: factory_deps
                    .iter()
                    .map(|b| BytecodeHash::for_bytecode(b).value_u256())
                    .collect(),
                paymaster_input: vec![],
                reserved_dynamic: vec![],
            }
            .into(),
            factory_deps,
        }
    }
}

impl TryFrom<abi::NewPriorityRequest> for L1Tx {
    type Error = anyhow::Error;

    /// Note that this method doesn't set `eth_block` and `received_timestamp_ms`
    /// because `req` doesn't contain those. They can be set after this conversion.
    fn try_from(req: abi::NewPriorityRequest) -> anyhow::Result<Self> {
        anyhow::ensure!(req.transaction.tx_type == PRIORITY_OPERATION_L2_TX_TYPE.into());
        anyhow::ensure!(req.transaction.nonce == req.tx_id); // serial id from decoded from transaction bytes should be equal to one from event
        anyhow::ensure!(req.transaction.max_priority_fee_per_gas == U256::zero());
        anyhow::ensure!(req.transaction.paymaster == U256::zero());
        anyhow::ensure!(req.transaction.hash() == H256::from_slice(&req.tx_hash));
        let factory_deps_hashes: Vec<_> = req
            .factory_deps
            .iter()
            .map(|b| BytecodeHash::for_bytecode(b).value_u256())
            .collect();
        anyhow::ensure!(req.transaction.factory_deps == factory_deps_hashes);
        for item in &req.transaction.reserved[2..] {
            anyhow::ensure!(item == &U256::zero());
        }
        anyhow::ensure!(req.transaction.signature.is_empty());
        anyhow::ensure!(req.transaction.paymaster_input.is_empty());
        anyhow::ensure!(req.transaction.reserved_dynamic.is_empty());

        let common_data = L1TxCommonData {
            serial_id: PriorityOpId(req.transaction.nonce.try_into().unwrap()),
            canonical_tx_hash: H256::from_slice(&req.tx_hash),
            sender: u256_to_address(&req.transaction.from),
            layer_2_tip_fee: U256::zero(),
            to_mint: req.transaction.reserved[0],
            refund_recipient: u256_to_address(&req.transaction.reserved[1]),
            full_fee: U256::zero(),
            gas_limit: req.transaction.gas_limit,
            max_fee_per_gas: req.transaction.max_fee_per_gas,
            gas_per_pubdata_limit: req.transaction.gas_per_pubdata_byte_limit,
            op_processing_type: OpProcessingType::Common,
            priority_queue_type: PriorityQueueType::Deque,
            // It set inside log convertion
            eth_block: 0,
        };

        let execute = Execute {
            contract_address: Some(u256_to_address(&req.transaction.to)),
            calldata: req.transaction.data,
            factory_deps: req.factory_deps,
            value: req.transaction.value,
        };
        Ok(Self {
            common_data,
            execute,
            received_timestamp_ms: 0,
        })
    }
}

impl TryFrom<Log> for L1Tx {
    type Error = L1TxParseError;

    fn try_from(event: Log) -> Result<Self, Self::Error> {
        let mut tx: L1Tx = abi::NewPriorityRequest::decode(&event.data.0)?
            .try_into()
            .map_err(|err| L1TxParseError::from(ethabi::Error::Other(format!("{err:#}").into())))?;
        tx.common_data.eth_block = event
            .block_number
            .expect("Event block number is missing")
            .try_into()
            .unwrap();
        tx.received_timestamp_ms = unix_timestamp_ms();
        Ok(tx)
    }
}

//! Definition of zkSync network priority operations: operations initiated from the L1.

use std::convert::TryFrom;

// use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use zksync_basic_types::{Address, L1BlockNumber, PriorityOpId, H160, H256, U256, U64};
// use zksync_crypto::hasher::{keccak::KeccakHasher, Hasher};
// use zksync_mini_merkle_tree::{compute_empty_tree_hashes, HashEmptySubtree};
use zksync_utils::address_to_u256;

use super::Transaction;
use crate::{
    abi,
    api,
    // abi, ethabi,
    api::TransactionRequest,
    fee::Fee,
    helpers::unix_timestamp_ms,
    l1::{OpProcessingType, PriorityQueueType},
    l2::TransactionType,
    priority_op_onchain_data::{PriorityOpOnchainData, PriorityOpOnchainMetadata},
    transaction_request::PaymasterParams,
    tx::Execute,
    web3::Bytes,
    ExecuteTransactionCommon, // INTEROP_TX_TYPE,
    InputData,
    // xl2::error::XL2TxParseError,
    Nonce,
    INTEROP_TX_TYPE,
};

pub mod error;

// TODO(PLA-962): remove once all nodes start treating the deprecated fields as optional.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct XL2TxCommonDataSerde {
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
    pub input: Option<InputData>,
    /// DEPRECATED.
    #[serde(default)]
    pub deadline_block: u64,
    #[serde(default)]
    pub eth_hash: H256,
    #[serde(default)]
    pub eth_block: u64,
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct XL2TxCommonData {
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
    /// Tx hash of the transaction in the zkSync network. Calculated as the encoded transaction data hash.
    pub canonical_tx_hash: H256,
    /// The amount of ETH that should be minted with this transaction
    pub to_mint: U256,
    /// The recipient of the refund of the transaction
    pub refund_recipient: Address,

    /// This input consists of raw transaction bytes when we receive it from API.    
    /// But we still use this structure for zksync-rs and tests, and we don't have raw tx before
    /// creating the structure. We setup this field manually later for consistency.    
    /// We need some research on how to change it
    pub input: Option<InputData>,

    // DEPRECATED.
    pub eth_block: u64,
}

impl serde::Serialize for XL2TxCommonData {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        XL2TxCommonDataSerde {
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
            input: self.input.clone(),
            // DEPRECATED.
            deadline_block: 0,
            eth_hash: H256::default(),
            eth_block: self.eth_block,
        }
        .serialize(s)
    }
}

impl<'de> serde::Deserialize<'de> for XL2TxCommonData {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let x = XL2TxCommonDataSerde::deserialize(d)?;
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
            input: x.input,
            // DEPRECATED.
            eth_block: x.eth_block,
        })
    }
}

impl XL2TxCommonData {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sender: Address,
        serial_id: PriorityOpId,
        layer_2_tip_fee: U256,
        full_fee: U256,
        max_fee_per_gas: U256,
        gas_limit: U256,
        gas_per_pubdata_limit: U256,
        op_processing_type: OpProcessingType,
        priority_queue_type: PriorityQueueType,
        canonical_tx_hash: H256,
        to_mint: U256,
        refund_recipient: Address,
        input: Option<InputData>,
    ) -> Self {
        Self {
            sender,
            serial_id,
            layer_2_tip_fee,
            full_fee,
            max_fee_per_gas,
            gas_limit,
            gas_per_pubdata_limit,
            op_processing_type,
            priority_queue_type,
            canonical_tx_hash,
            to_mint,
            refund_recipient,
            input,
            eth_block: 0,
        }
    }

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
        TransactionType::InteropTransaction
    }

    pub fn input_data(&self) -> Option<&[u8]> {
        self.input.as_ref().map(|input| &*input.data)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct XL2Tx {
    pub execute: Execute,
    pub common_data: XL2TxCommonData,
    pub received_timestamp_ms: u64,
}

// impl HashEmptySubtree<XL2Tx> for KeccakHasher {
//     fn empty_subtree_hash(&self, depth: usize) -> H256 {
//         static EMPTY_HASHES: OnceCell<Vec<H256>> = OnceCell::new();
//         EMPTY_HASHES.get_or_init(|| compute_empty_tree_hashes(self.hash_bytes(&[])))[depth]
//     }
// }

impl From<XL2Tx> for TransactionRequest {
    fn from(tx: XL2Tx) -> Self {
        let tx_type = INTEROP_TX_TYPE as u32;
        // let (v, r, s) = signature_to_vrs(&tx.common_data.signature, tx_type);

        let mut base_tx_req = TransactionRequest {
            nonce: U256::from(tx.common_data.serial_id.0),
            from: Some(tx.common_data.sender),
            to: Some(tx.execute.contract_address),
            value: tx.execute.value,
            gas_price: tx.common_data.max_fee_per_gas,
            max_priority_fee_per_gas: Some(U256::from(0)),
            gas: tx.common_data.gas_limit,
            input: Bytes(tx.execute.calldata),
            v: None,
            r: None,
            s: None,
            raw: None,
            transaction_type: None,
            access_list: None,
            eip712_meta: None,
            chain_id: Some(270), // todo
            merkle_proof: None,
            full_fee: Some(tx.common_data.full_fee),
            to_mint: Some(tx.common_data.to_mint),
            refund_recipient: Some(tx.common_data.refund_recipient),
        };

        base_tx_req.transaction_type = Some(U64::from(tx_type));
        base_tx_req.max_priority_fee_per_gas = Some(U256::from(0));
        base_tx_req.eip712_meta = Some(api::Eip712Meta {
            gas_per_pubdata: tx.common_data.gas_per_pubdata_limit,
            factory_deps: tx.execute.factory_deps,
            custom_signature: None,
            paymaster_params: Some(PaymasterParams {
                paymaster: H160::default(),
                paymaster_input: vec![],
            }),
        });
        base_tx_req
    }
}

impl From<XL2Tx> for Transaction {
    fn from(tx: XL2Tx) -> Self {
        let XL2Tx {
            execute,
            common_data,
            received_timestamp_ms,
        } = tx;
        Self {
            common_data: ExecuteTransactionCommon::XL2(common_data),
            execute,
            received_timestamp_ms,
            raw_bytes: None,
        }
    }
}

impl From<XL2Tx> for api::Transaction {
    fn from(tx: XL2Tx) -> Self {
        // let tx_type = INTEROP_TX_TYPE as u32;
        let (v, r, s) = (None, None, None);

        Self {
            hash: tx.hash(),
            chain_id: U256::from(270), // todo
            nonce: U256::from(0),
            from: Some(tx.common_data.sender),
            to: Some(tx.execute.contract_address),
            value: tx.execute.value,
            gas_price: Some(tx.common_data.max_fee_per_gas),
            max_priority_fee_per_gas: Some(U256::from(0)), // Some(tx.common_data.max_priority_fee_per_gas),
            max_fee_per_gas: Some(tx.common_data.max_fee_per_gas),
            gas: tx.common_data.gas_limit,
            input: Bytes(tx.execute.calldata),
            v,
            r,
            s,
            transaction_type: Some(U64([INTEROP_TX_TYPE as u64])),
            ..Default::default()
        }
    }
}

impl From<XL2Tx> for abi::L2CanonicalTransaction {
    fn from(tx: XL2Tx) -> Self {
        let XL2Tx {
            execute,
            common_data,
            received_timestamp_ms: _,
        } = tx;
        Self {
            tx_type: INTEROP_TX_TYPE.into(),
            from: address_to_u256(&common_data.sender),
            to: address_to_u256(&execute.contract_address),
            gas_limit: common_data.gas_limit,
            gas_per_pubdata_byte_limit: common_data.gas_per_pubdata_limit,
            max_fee_per_gas: common_data.max_fee_per_gas,
            max_priority_fee_per_gas: U256::from(0),
            paymaster: U256::from(0),
            nonce: common_data.serial_id.0.into(),
            value: execute.value,
            reserved: [
                common_data.to_mint,
                address_to_u256(&common_data.refund_recipient),
                0.into(),
                0.into(),
            ],
            data: execute.calldata,
            signature: Default::default(),
            factory_deps: Default::default(), // execute.factory_deps, // kl todo
            paymaster_input: Default::default(),
            reserved_dynamic: Default::default(),
        }
    }
}

impl TryFrom<Transaction> for XL2Tx {
    type Error = &'static str;

    fn try_from(value: Transaction) -> Result<Self, Self::Error> {
        let Transaction {
            common_data,
            execute,
            received_timestamp_ms,
            ..
        } = value;
        match common_data {
            ExecuteTransactionCommon::XL2(common_data) => Ok(XL2Tx {
                execute,
                common_data,
                received_timestamp_ms,
            }),
            ExecuteTransactionCommon::L1(_) => Err("Cannot convert L1Tx to XL2Tx"),
            ExecuteTransactionCommon::L2(_) => Err("Cannot convert L2Tx to XL2Tx"),
            ExecuteTransactionCommon::ProtocolUpgrade(_) => {
                Err("Cannot convert ProtocolUpgradeTx to XL2Tx")
            }
        }
    }
}

impl XL2Tx {
    pub fn serial_id(&self) -> PriorityOpId {
        self.common_data.serial_id
    }

    pub fn eth_block(&self) -> L1BlockNumber {
        L1BlockNumber(self.common_data.eth_block as u32)
    }

    pub fn hash(&self) -> H256 {
        self.common_data.hash()
    }

    pub fn set_input(&mut self, input: Vec<u8>, hash: H256) {
        self.common_data.input = Some(InputData { hash, data: input })
    }

    pub fn abi_encoding_len(&self) -> usize {
        // let data_len = self.execute.calldata.len();
        // let signature_len = self.common_data.signature.len();
        // let factory_deps_len = self.execute.factory_deps.len();
        // let paymaster_input_len = self.common_data.paymaster_params.paymaster_input.len();

        0 //todo
          // encoding_len(
          //     data_len as u64,
          //     signature_len as u64,
          //     factory_deps_len as u64,
          //     paymaster_input_len as u64,
          //     0,
          // )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        contract_address: Address,
        calldata: Vec<u8>,
        nonce: Nonce,
        fee: Fee,
        full_fee: U256,
        initiator_address: Address,
        value: U256,
        factory_deps: Vec<Vec<u8>>,
        // paymaster_params: PaymasterParams,
    ) -> Self {
        // println!("kl todo new xl2 tx");
        Self {
            execute: Execute {
                contract_address,
                calldata,
                value,
                factory_deps,
            },
            // todo
            common_data: XL2TxCommonData {
                sender: initiator_address,
                serial_id: PriorityOpId(nonce.0 as u64),
                layer_2_tip_fee: Default::default(),
                full_fee,
                max_fee_per_gas: fee.max_fee_per_gas,
                gas_limit: fee.gas_limit,
                gas_per_pubdata_limit: fee.gas_per_pubdata_limit,
                op_processing_type: Default::default(),
                priority_queue_type: Default::default(),
                canonical_tx_hash: Default::default(),
                to_mint: Default::default(),
                refund_recipient: Default::default(),
                input: None,
                eth_block: 0,
            },
            received_timestamp_ms: unix_timestamp_ms(),
        }
    }
}

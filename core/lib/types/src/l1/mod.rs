//! Definition of zkSync network priority operations: operations initiated from the L1.

use std::convert::TryFrom;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use zksync_basic_types::{
    ethabi::{self, ParamType, Token},
    web3::{self, Log},
    Address, L1BlockNumber, PriorityOpId, H256, U256,
};
use zksync_utils::{
    address_to_u256, bytecode::hash_bytecode, h256_to_u256, u256_to_account_address,
};

use super::Transaction;
use crate::{
    helpers::unix_timestamp_ms,
    l1::error::L1TxParseError,
    l2::TransactionType,
    priority_op_onchain_data::{PriorityOpOnchainData, PriorityOpOnchainMetadata},
    tx::Execute,
    ExecuteTransactionCommon, PRIORITY_OPERATION_L2_TX_TYPE, PROTOCOL_UPGRADE_TX_TYPE,
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

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct L1TxCommonData {
    /// Sender of the transaction.
    pub sender: Address,
    /// Unique ID of the priority operation.
    pub serial_id: PriorityOpId,
    /// Ethereum deadline block until which operation must be processed.
    pub deadline_block: u64,
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
    /// Hash of the corresponding Ethereum transaction. Size should be 32 bytes.
    pub eth_hash: H256,
    /// Block in which Ethereum transaction was included.
    pub eth_block: u64,
    /// Tx hash of the transaction in the zkSync network. Calculated as the encoded transaction data hash.
    pub canonical_tx_hash: H256,
    /// The amount of ETH that should be minted with this transaction
    pub to_mint: U256,
    /// The recipient of the refund of the transaction
    pub refund_recipient: Address,
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

pub struct NewPriorityRequest {
    tx_id: U256,
    tx_hash: [u8; 32],
    expiration_timestamp: u64,
    transaction: L2CanonicalTransaction,
    factory_deps: Vec<Vec<u8>>,
}

pub struct L2CanonicalTransaction {
    tx_type: U256,
    from: U256,
    to: U256,
    gas_limit: U256,
    gas_per_pubdata_byte_limit: U256,
    max_fee_per_gas: U256,
    max_priority_fee_per_gas: U256,
    paymaster: U256,
    nonce: U256,
    value: U256,
    reserved: [U256; 4],
    data: Vec<u8>,
    signature: Vec<u8>,
    factory_deps: Vec<U256>,
    paymaster_input: Vec<u8>,
    reserved_dynamic: Vec<u8>,
}

impl NewPriorityRequest {
    pub fn schema() -> Vec<ParamType> {
        vec![
            ParamType::Uint(256),                               // tx ID
            ParamType::FixedBytes(32),                          // tx hash
            ParamType::Uint(64),                                // expiration block
            ParamType::Tuple(L2CanonicalTransaction::schema()), // transaction data
            ParamType::Array(ParamType::Bytes.into()),          // factory deps
        ]
    }

    pub fn decode(tokens: Vec<Token>) -> anyhow::Result<Self> {
        anyhow::ensure!(tokens.len() == 5);
        let mut t = tokens.into_iter();
        let mut next = || t.next().unwrap();
        Ok(Self {
            tx_id: next().into_uint().context("tx_id")?,
            tx_hash: next()
                .into_fixed_bytes()
                .and_then(|x| x.try_into().ok())
                .context("tx_hash")?,
            expiration_timestamp: next()
                .into_uint()
                .and_then(|x| x.try_into().ok())
                .context("expiration_timestamp")?,
            transaction: L2CanonicalTransaction::decode(
                next().into_tuple().context("transaction")?,
            )
            .context("transaction")?,
            factory_deps: next()
                .into_array()
                .context("factory_deps")?
                .into_iter()
                .enumerate()
                .map(|(i, t)| t.into_bytes().context(i))
                .collect::<Result<_, _>>()
                .context("factory_deps")?,
        })
    }

    pub fn encode(&self) -> Vec<Token> {
        vec![
            Token::Uint(self.tx_id),
            Token::FixedBytes(self.tx_hash.into()),
            Token::Uint(self.expiration_timestamp.into()),
            Token::Tuple(self.transaction.encode()),
            Token::Array(
                self.factory_deps
                    .iter()
                    .map(|b| Token::Bytes(b.clone()))
                    .collect(),
            ),
        ]
    }
}

impl L2CanonicalTransaction {
    pub fn schema() -> Vec<ParamType> {
        // TODO: refactor according to tx type
        vec![
            ParamType::Uint(8),                                    // `txType`
            ParamType::Uint(256),                                  // sender
            ParamType::Uint(256),                                  // to
            ParamType::Uint(256),                                  // gasLimit
            ParamType::Uint(256),                                  // `gasPerPubdataLimit`
            ParamType::Uint(256),                                  // maxFeePerGas
            ParamType::Uint(256),                                  // maxPriorityFeePerGas
            ParamType::Uint(256),                                  // paymaster
            ParamType::Uint(256),                                  // nonce (serial ID)
            ParamType::Uint(256),                                  // value
            ParamType::FixedArray(ParamType::Uint(256).into(), 4), // reserved
            ParamType::Bytes,                                      // calldata
            ParamType::Bytes,                                      // signature
            ParamType::Array(Box::new(ParamType::Uint(256))),      // factory deps
            ParamType::Bytes,                                      // paymaster input
            ParamType::Bytes,                                      // `reservedDynamic`
        ]
    }

    pub fn decode(tokens: Vec<Token>) -> anyhow::Result<Self> {
        anyhow::ensure!(tokens.len() == 16);
        let mut t = tokens.into_iter();
        let mut next = || t.next().unwrap();
        Ok(Self {
            tx_type: next().into_uint().context("tx_type")?,
            from: next().into_uint().context("from")?,
            to: next().into_uint().context("to")?,
            gas_limit: next().into_uint().context("gas_limit")?,
            gas_per_pubdata_byte_limit: next().into_uint().context("gas_per_pubdata_byte_limit")?,
            max_fee_per_gas: next().into_uint().context("max_fee_per_gas")?,
            max_priority_fee_per_gas: next().into_uint().context("max_priority_fee_per_gas")?,
            paymaster: next().into_uint().context("paymaster")?,
            nonce: next().into_uint().context("nonce")?,
            value: next().into_uint().context("value")?,
            reserved: next()
                .into_fixed_array()
                .context("reserved")?
                .into_iter()
                .enumerate()
                .map(|(i, t)| t.into_uint().context(i))
                .collect::<Result<Vec<_>, _>>()
                .context("reserved")?
                .try_into()
                .ok()
                .context("reserved")?,
            data: next().into_bytes().context("data")?,
            signature: next().into_bytes().context("signature")?,
            factory_deps: next()
                .into_array()
                .context("factory_deps")?
                .into_iter()
                .enumerate()
                .map(|(i, t)| t.into_uint().context(i))
                .collect::<Result<_, _>>()
                .context("factory_deps")?,
            paymaster_input: next().into_bytes().context("paymaster_input")?,
            reserved_dynamic: next().into_bytes().context("reserved_dynamic")?,
        })
    }

    pub fn encode(&self) -> Vec<Token> {
        vec![
            Token::Uint(self.tx_type),
            Token::Uint(self.from),
            Token::Uint(self.to),
            Token::Uint(self.gas_limit),
            Token::Uint(self.gas_per_pubdata_byte_limit),
            Token::Uint(self.max_fee_per_gas),
            Token::Uint(self.max_priority_fee_per_gas),
            Token::Uint(self.paymaster),
            Token::Uint(self.nonce),
            Token::Uint(self.value),
            Token::FixedArray(self.reserved.iter().map(|x| Token::Uint(*x)).collect()),
            Token::Bytes(self.data.clone()),
            Token::Bytes(self.signature.clone()),
            Token::Array(self.factory_deps.iter().map(|x| Token::Uint(*x)).collect()),
            Token::Bytes(self.paymaster_input.clone()),
            Token::Bytes(self.reserved_dynamic.clone()),
        ]
    }

    pub fn hash(&self) -> H256 {
        H256::from_slice(&web3::keccak256(&ethabi::encode(&self.encode())))
    }
}

impl From<L1Tx> for NewPriorityRequest {
    fn from(t: L1Tx) -> Self {
        let factory_deps = t.execute.factory_deps.unwrap_or_default();
        Self {
            tx_id: t.common_data.serial_id.0.into(),
            tx_hash: t.common_data.canonical_tx_hash.to_fixed_bytes(),
            expiration_timestamp: t.common_data.deadline_block,
            transaction: L2CanonicalTransaction {
                tx_type: PRIORITY_OPERATION_L2_TX_TYPE.into(),
                from: address_to_u256(&t.common_data.sender),
                to: address_to_u256(&t.execute.contract_address),
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
                    .map(|b| h256_to_u256(hash_bytecode(b)))
                    .collect(),
                paymaster_input: vec![],
                reserved_dynamic: vec![],
            },
            factory_deps,
        }
    }
}

impl TryFrom<NewPriorityRequest> for L1Tx {
    type Error = L1TxParseError;

    fn try_from(req: NewPriorityRequest) -> Result<Self, Self::Error> {
        assert_eq!(
            req.transaction.tx_type,
            PRIORITY_OPERATION_L2_TX_TYPE.into()
        );
        assert_eq!(req.transaction.nonce, req.tx_id); // serial id from decoded from transaction bytes should be equal to one from event
        assert_eq!(req.transaction.max_priority_fee_per_gas, U256::zero());
        assert_eq!(req.transaction.paymaster, U256::zero());
        assert_eq!(req.transaction.hash(), H256::from_slice(&req.tx_hash));
        let factory_deps_hashes: Vec<_> = req
            .factory_deps
            .iter()
            .map(|b| h256_to_u256(hash_bytecode(b)))
            .collect();
        assert_eq!(req.transaction.factory_deps, factory_deps_hashes);
        for item in &req.transaction.reserved[2..] {
            assert_eq!(item, &U256::zero());
        }
        assert!(req.transaction.signature.is_empty());
        // TODO (SMA-1621): check that `reservedDynamic` are constructed correctly.
        assert!(req.transaction.paymaster_input.is_empty());
        assert!(req.transaction.reserved_dynamic.is_empty());

        let common_data = L1TxCommonData {
            serial_id: PriorityOpId(req.transaction.nonce.try_into().unwrap()),
            canonical_tx_hash: H256::from_slice(&req.tx_hash),
            sender: u256_to_account_address(&req.transaction.from),
            deadline_block: req.expiration_timestamp,
            layer_2_tip_fee: U256::zero(),
            to_mint: req.transaction.reserved[0],
            refund_recipient: u256_to_account_address(&req.transaction.reserved[1]),
            full_fee: U256::zero(),
            gas_limit: req.transaction.gas_limit,
            max_fee_per_gas: req.transaction.max_fee_per_gas,
            gas_per_pubdata_limit: req.transaction.gas_per_pubdata_byte_limit,
            op_processing_type: OpProcessingType::Common,
            priority_queue_type: PriorityQueueType::Deque,
            eth_hash: H256::default(),
            eth_block: 0,
        };

        let execute = Execute {
            contract_address: u256_to_account_address(&req.transaction.to),
            calldata: req.transaction.data,
            factory_deps: Some(req.factory_deps),
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
        let tokens = ethabi::decode(&NewPriorityRequest::schema(), &event.data.0)?;
        let mut tx: L1Tx = NewPriorityRequest::decode(tokens).unwrap().try_into()?;
        tx.common_data.eth_hash = event
            .transaction_hash
            .expect("Event transaction hash is missing");
        tx.common_data.eth_block = event
            .block_number
            .expect("Event block number is missing")
            .try_into()
            .unwrap();
        tx.received_timestamp_ms = unix_timestamp_ms();
        Ok(tx)
    }
}

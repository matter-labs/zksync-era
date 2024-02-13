use anyhow::{anyhow, Context as _};
use zksync_consensus_roles::validator;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_protobuf::{required, ProtoFmt};
use zksync_protobuf_config::repr::ProtoRepr;
use zksync_types::{
    api::en,
    fee::Fee,
    l1::{OpProcessingType, PriorityQueueType},
    l2::TransactionType,
    protocol_version::ProtocolUpgradeTxCommonData,
    transaction_request::PaymasterParams,
    Address, Execute, ExecuteTransactionCommon, InputData, L1BatchNumber, L1TxCommonData,
    L2TxCommonData, MiniblockNumber, Nonce, PriorityOpId, ProtocolVersionId, Transaction, H160,
    H256,
};
use zksync_utils::{h256_to_u256, u256_to_h256};

use crate::models::proto;

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct StorageSyncBlock {
    pub number: i64,
    pub l1_batch_number: i64,
    pub last_batch_miniblock: Option<i64>,
    pub timestamp: i64,
    // L1 gas price assumed in the corresponding batch
    pub l1_gas_price: i64,
    // L2 gas price assumed in the corresponding batch
    pub l2_fair_gas_price: i64,
    pub fair_pubdata_price: Option<i64>,
    pub bootloader_code_hash: Option<Vec<u8>>,
    pub default_aa_code_hash: Option<Vec<u8>>,
    pub fee_account_address: Vec<u8>,
    pub protocol_version: i32,
    pub virtual_blocks: i64,
    pub hash: Vec<u8>,
}

fn parse_h256(bytes: &[u8]) -> anyhow::Result<H256> {
    Ok(<[u8; 32]>::try_from(bytes).context("invalid size")?.into())
}

fn parse_h160(bytes: &[u8]) -> anyhow::Result<H160> {
    Ok(<[u8; 20]>::try_from(bytes).context("invalid size")?.into())
}

pub(crate) struct SyncBlock {
    pub number: MiniblockNumber,
    pub l1_batch_number: L1BatchNumber,
    pub last_in_batch: bool,
    pub timestamp: u64,
    pub l1_gas_price: u64,
    pub l2_fair_gas_price: u64,
    pub fair_pubdata_price: Option<u64>,
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
    pub fee_account_address: Address,
    pub virtual_blocks: u32,
    pub hash: H256,
    pub protocol_version: ProtocolVersionId,
}

impl TryFrom<StorageSyncBlock> for SyncBlock {
    type Error = anyhow::Error;
    fn try_from(block: StorageSyncBlock) -> anyhow::Result<Self> {
        Ok(Self {
            number: MiniblockNumber(block.number.try_into().context("number")?),
            l1_batch_number: L1BatchNumber(
                block
                    .l1_batch_number
                    .try_into()
                    .context("l1_batch_number")?,
            ),
            last_in_batch: block.last_batch_miniblock == Some(block.number),
            timestamp: block.timestamp.try_into().context("timestamp")?,
            l1_gas_price: block.l1_gas_price.try_into().context("l1_gas_price")?,
            l2_fair_gas_price: block
                .l2_fair_gas_price
                .try_into()
                .context("l2_fair_gas_price")?,
            fair_pubdata_price: block
                .fair_pubdata_price
                .map(|v| v.try_into().context("fair_pubdata_price"))
                .transpose()?,
            // TODO (SMA-1635): Make these fields non optional in database
            base_system_contracts_hashes: BaseSystemContractsHashes {
                bootloader: parse_h256(
                    &block
                        .bootloader_code_hash
                        .context("bootloader_code_hash should not be none")?,
                )
                .context("bootloader_code_hash")?,
                default_aa: parse_h256(
                    &block
                        .default_aa_code_hash
                        .context("default_aa_code_hash should not be none")?,
                )
                .context("default_aa_code_hash")?,
            },
            fee_account_address: parse_h160(&block.fee_account_address)
                .context("fee_account_address")?,
            virtual_blocks: block.virtual_blocks.try_into().context("virtual_blocks")?,
            hash: parse_h256(&block.hash).context("hash")?,
            protocol_version: u16::try_from(block.protocol_version)
                .context("protocol_version")?
                .try_into()
                .context("protocol_version")?,
        })
    }
}

impl SyncBlock {
    pub(crate) fn into_api(self, transactions: Option<Vec<Transaction>>) -> en::SyncBlock {
        en::SyncBlock {
            number: self.number,
            l1_batch_number: self.l1_batch_number,
            last_in_batch: self.last_in_batch,
            timestamp: self.timestamp,
            l1_gas_price: self.l1_gas_price,
            l2_fair_gas_price: self.l2_fair_gas_price,
            fair_pubdata_price: self.fair_pubdata_price,
            base_system_contracts_hashes: self.base_system_contracts_hashes,
            operator_address: self.fee_account_address,
            transactions,
            virtual_blocks: Some(self.virtual_blocks),
            hash: Some(self.hash),
            protocol_version: self.protocol_version,
        }
    }

    pub(crate) fn into_payload(self, transactions: Vec<Transaction>) -> Payload {
        Payload {
            protocol_version: self.protocol_version,
            hash: self.hash,
            l1_batch_number: self.l1_batch_number,
            timestamp: self.timestamp,
            l1_gas_price: self.l1_gas_price,
            l2_fair_gas_price: self.l2_fair_gas_price,
            fair_pubdata_price: self.fair_pubdata_price,
            virtual_blocks: self.virtual_blocks,
            operator_address: self.fee_account_address,
            transactions,
            last_in_batch: self.last_in_batch,
        }
    }
}

/// L2 block (= miniblock) payload.
#[derive(Debug, PartialEq)]
pub struct Payload {
    pub protocol_version: ProtocolVersionId,
    pub hash: H256,
    pub l1_batch_number: L1BatchNumber,
    pub timestamp: u64,
    pub l1_gas_price: u64,
    pub l2_fair_gas_price: u64,
    pub fair_pubdata_price: Option<u64>,
    pub virtual_blocks: u32,
    pub operator_address: Address,
    pub transactions: Vec<Transaction>,
    pub last_in_batch: bool,
}

impl ProtoFmt for Payload {
    type Proto = super::proto::Payload;

    fn read(message: &Self::Proto) -> anyhow::Result<Self> {
        let mut transactions = Vec::with_capacity(message.transactions.len());
        for tx in message.transactions.iter() {
            transactions.push(tx.read()?)
        }

        Ok(Self {
            protocol_version: required(&message.protocol_version)
                .and_then(|x| Ok(ProtocolVersionId::try_from(u16::try_from(*x)?)?))
                .context("protocol_version")?,
            hash: required(&message.hash)
                .and_then(|h| parse_h256(h))
                .context("hash")?,
            l1_batch_number: L1BatchNumber(
                *required(&message.l1_batch_number).context("l1_batch_number")?,
            ),
            timestamp: *required(&message.timestamp).context("timestamp")?,
            l1_gas_price: *required(&message.l1_gas_price).context("l1_gas_price")?,
            l2_fair_gas_price: *required(&message.l2_fair_gas_price)
                .context("l2_fair_gas_price")?,
            fair_pubdata_price: message.fair_pubdata_price,
            virtual_blocks: *required(&message.virtual_blocks).context("virtual_blocks")?,
            operator_address: required(&message.operator_address)
                .and_then(|a| parse_h160(a))
                .context("operator_address")?,
            transactions,
            last_in_batch: *required(&message.last_in_batch).context("last_in_batch")?,
        })
    }

    fn build(&self) -> Self::Proto {
        Self::Proto {
            protocol_version: Some((self.protocol_version as u16).into()),
            hash: Some(self.hash.as_bytes().into()),
            l1_batch_number: Some(self.l1_batch_number.0),
            timestamp: Some(self.timestamp),
            l1_gas_price: Some(self.l1_gas_price),
            l2_fair_gas_price: Some(self.l2_fair_gas_price),
            fair_pubdata_price: self.fair_pubdata_price,
            virtual_blocks: Some(self.virtual_blocks),
            operator_address: Some(self.operator_address.as_bytes().into()),
            // Transactions are stored in execution order, therefore order is deterministic.
            transactions: self
                .transactions
                .iter()
                .map(|t| proto::Transaction::build(t))
                .collect(),
            last_in_batch: Some(self.last_in_batch),
        }
    }
}

impl Payload {
    pub fn decode(payload: &validator::Payload) -> anyhow::Result<Self> {
        zksync_protobuf::decode(&payload.0)
    }

    pub fn encode(&self) -> validator::Payload {
        validator::Payload(zksync_protobuf::encode(self))
    }
}

impl ProtoRepr for proto::Transaction {
    type Type = Transaction;

    fn read(&self) -> anyhow::Result<Self::Type> {
        let common_data = required(&self.common_data).context("common_data")?;
        let execute = required(&self.execute).context("execute")?;
        Ok(Self::Type {
            common_data: match common_data {
                proto::transaction::CommonData::L1(data) => {
                    ExecuteTransactionCommon::L1(L1TxCommonData {
                        sender: required(&data.sender_address)
                            .and_then(|x| parse_h160(x))
                            .context("common_data.sender_address")?,
                        serial_id: required(&data.serial_id)
                            .map(|x| PriorityOpId(*x))
                            .context("common_data.serial_id")?,
                        deadline_block: *required(&data.deadline_block)
                            .context("common_data.deadline_block")?,
                        layer_2_tip_fee: required(&data.layer_2_tip_fee)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("common_data.layer_2_tip_fee")?,
                        full_fee: required(&data.full_fee)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("common_data.full_fee")?,
                        max_fee_per_gas: required(&data.max_fee_per_gas)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("common_data.max_fee_per_gas")?,
                        gas_limit: required(&data.gas_limit)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("common_data.gas_limit")?,
                        gas_per_pubdata_limit: required(&data.gas_per_pubdata_limit)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("common_data.gas_per_pubdata_limit")?,
                        op_processing_type: required(&data.op_processing_type)
                            .and_then(|x| {
                                OpProcessingType::try_from(u8::try_from(*x)?)
                                    .map_err(|_| anyhow!("u8::try_from"))
                            })
                            .context("common_data.op_processing_type")?,
                        priority_queue_type: required(&data.priority_queue_type)
                            .and_then(|x| {
                                PriorityQueueType::try_from(u8::try_from(*x)?)
                                    .map_err(|_| anyhow!("u8::try_from"))
                            })
                            .context("common_data.priority_queue_type")?,
                        eth_hash: required(&data.eth_hash)
                            .and_then(|x| parse_h256(x))
                            .context("common_data.eth_hash")?,
                        eth_block: *required(&data.eth_block).context("common_data.eth_block")?,
                        canonical_tx_hash: required(&data.canonical_tx_hash)
                            .and_then(|x| parse_h256(x))
                            .context("common_data.canonical_tx_hash")?,
                        to_mint: required(&data.to_mint)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("common_data.to_mint")?,
                        refund_recipient: required(&data.refund_recipient_address)
                            .and_then(|x| parse_h160(x))
                            .context("common_data.refund_recipient_address")?,
                    })
                }
                proto::transaction::CommonData::L2(data) => {
                    ExecuteTransactionCommon::L2(L2TxCommonData {
                        nonce: required(&data.nonce)
                            .map(|x| Nonce(*x))
                            .context("common_data.nonce")?,
                        fee: Fee {
                            gas_limit: required(&data.gas_limit)
                                .and_then(|x| parse_h256(x))
                                .map(h256_to_u256)
                                .context("common_data.gas_limit")?,
                            max_fee_per_gas: required(&data.max_fee_per_gas)
                                .and_then(|x| parse_h256(x))
                                .map(h256_to_u256)
                                .context("common_data.max_fee_per_gas")?,
                            max_priority_fee_per_gas: required(&data.max_priority_fee_per_gas)
                                .and_then(|x| parse_h256(x))
                                .map(h256_to_u256)
                                .context("common_data.max_priority_fee_per_gas")?,
                            gas_per_pubdata_limit: required(&data.gas_per_pubdata_limit)
                                .and_then(|x| parse_h256(x))
                                .map(h256_to_u256)
                                .context("common_data.gas_per_pubdata_limit")?,
                        },
                        initiator_address: required(&data.initiator_address)
                            .and_then(|x| parse_h160(x))
                            .context("common_data.initiator_address")?,
                        signature: required(&data.signature)
                            .context("common_data.signature")?
                            .clone(),
                        transaction_type: required(&data.transaction_type)
                            .and_then(|x| Ok(TransactionType::try_from(u32::try_from(*x)?)?))
                            .context("common_data.transaction_type")?,
                        input: {
                            match &data.input {
                                None => None,
                                Some(input) => Some(InputData {
                                    hash: required(&input.hash)
                                        .and_then(|x| parse_h256(x))
                                        .context("common_data.input.hash")?,
                                    data: required(&input.data)
                                        .context("common_data.input.data")?
                                        .clone(),
                                }),
                            }
                        },
                        paymaster_params: {
                            let params = required(&data.paymaster_params)?;
                            PaymasterParams {
                                paymaster: required(&params.paymaster_address)
                                    .and_then(|x| parse_h160(x))
                                    .context("common_data.paymaster_params.paymaster_address")?,
                                paymaster_input: required(&params.paymaster_input)
                                    .context("common_data.paymaster_params.paymaster_input")?
                                    .clone(),
                            }
                        },
                    })
                }
                proto::transaction::CommonData::ProtocolUpgrade(data) => {
                    ExecuteTransactionCommon::ProtocolUpgrade(ProtocolUpgradeTxCommonData {
                        sender: required(&data.sender_address)
                            .and_then(|x| parse_h160(x))
                            .context("common_data.sender_address")?,
                        upgrade_id: required(&data.upgrade_id)
                            .and_then(|x| Ok(ProtocolVersionId::try_from(u16::try_from(*x)?)?))
                            .context("protocol_version")?,
                        max_fee_per_gas: required(&data.max_fee_per_gas)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("max_fee_per_gas")?,
                        gas_limit: required(&data.gas_limit)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("gas_limit")?,
                        gas_per_pubdata_limit: required(&data.gas_per_pubdata_limit)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("gas_per_pubdata_limit")?,
                        eth_hash: required(&data.eth_hash)
                            .and_then(|x| parse_h256(x))
                            .context("eth_hash")?,
                        eth_block: *required(&data.eth_block).context("common_data.eth_block")?,
                        canonical_tx_hash: required(&data.canonical_tx_hash)
                            .and_then(|x| parse_h256(x))
                            .context("eth_hash")?,
                        to_mint: required(&data.to_mint)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("max_fee_per_gas")?,
                        refund_recipient: required(&data.refund_recipient_address)
                            .and_then(|x| parse_h160(x))
                            .context("common_data.sender_address")?,
                    })
                }
            },
            execute: Execute {
                contract_address: required(&execute.contract_address)
                    .and_then(|x| parse_h160(x))
                    .context("contract_address")?,
                calldata: required(&execute.calldata).context("calldata")?.clone(),
                value: required(&execute.value)
                    .and_then(|x| parse_h256(x))
                    .map(h256_to_u256)
                    .context("contract_address")?,
                factory_deps: match execute.factory_deps.is_empty() {
                    true => None,
                    false => Some(execute.factory_deps.clone()),
                },
            },
            received_timestamp_ms: *required(&self.received_timestamp_ms)
                .context("received_timestamp_ms")?,
            raw_bytes: self.raw_bytes.as_ref().map(|x| x.clone().into()),
        })
    }

    fn build(this: &Self::Type) -> Self {
        let common_data = match &this.common_data {
            ExecuteTransactionCommon::L1(data) => {
                proto::transaction::CommonData::L1(proto::L1TxCommonData {
                    sender_address: Some(data.sender.as_bytes().into()),
                    serial_id: Some(data.serial_id.0),
                    deadline_block: Some(data.deadline_block),
                    layer_2_tip_fee: Some(u256_to_h256(data.layer_2_tip_fee).as_bytes().into()),
                    full_fee: Some(u256_to_h256(data.full_fee).as_bytes().into()),
                    max_fee_per_gas: Some(u256_to_h256(data.max_fee_per_gas).as_bytes().into()),
                    gas_limit: Some(u256_to_h256(data.gas_limit).as_bytes().into()),
                    gas_per_pubdata_limit: Some(
                        u256_to_h256(data.gas_per_pubdata_limit).as_bytes().into(),
                    ),
                    op_processing_type: Some(data.op_processing_type as u32),
                    priority_queue_type: Some(data.priority_queue_type as u32),
                    eth_hash: Some(data.eth_hash.as_bytes().into()),
                    eth_block: Some(data.eth_block),
                    canonical_tx_hash: Some(data.canonical_tx_hash.as_bytes().into()),
                    to_mint: Some(u256_to_h256(data.to_mint).as_bytes().into()),
                    refund_recipient_address: Some(data.refund_recipient.as_bytes().into()),
                })
            }
            ExecuteTransactionCommon::L2(data) => {
                proto::transaction::CommonData::L2(proto::L2TxCommonData {
                    nonce: Some(data.nonce.0),
                    gas_limit: Some(u256_to_h256(data.fee.gas_limit).as_bytes().into()),
                    max_fee_per_gas: Some(u256_to_h256(data.fee.max_fee_per_gas).as_bytes().into()),
                    max_priority_fee_per_gas: Some(
                        u256_to_h256(data.fee.max_priority_fee_per_gas)
                            .as_bytes()
                            .into(),
                    ),
                    gas_per_pubdata_limit: Some(
                        u256_to_h256(data.fee.gas_per_pubdata_limit)
                            .as_bytes()
                            .into(),
                    ),
                    initiator_address: Some(data.initiator_address.as_bytes().into()),
                    signature: Some(data.signature.clone()),
                    transaction_type: Some(data.transaction_type as u32),
                    input: data.input.as_ref().map(|input_data| proto::InputData {
                        data: Some(input_data.data.clone()),
                        hash: Some(input_data.hash.as_bytes().into()),
                    }),
                    paymaster_params: Some(proto::PaymasterParams {
                        paymaster_input: Some(data.paymaster_params.paymaster_input.clone()),
                        paymaster_address: Some(data.paymaster_params.paymaster.as_bytes().into()),
                    }),
                })
            }
            ExecuteTransactionCommon::ProtocolUpgrade(data) => {
                proto::transaction::CommonData::ProtocolUpgrade(
                    proto::ProtocolUpgradeTxCommonData {
                        sender_address: Some(data.sender.as_bytes().into()),
                        upgrade_id: Some(data.upgrade_id as u32),
                        max_fee_per_gas: Some(u256_to_h256(data.max_fee_per_gas).as_bytes().into()),
                        gas_limit: Some(u256_to_h256(data.gas_limit).as_bytes().into()),
                        gas_per_pubdata_limit: Some(
                            u256_to_h256(data.gas_per_pubdata_limit).as_bytes().into(),
                        ),
                        eth_hash: Some(data.eth_hash.as_bytes().into()),
                        eth_block: Some(data.eth_block),
                        canonical_tx_hash: Some(data.canonical_tx_hash.as_bytes().into()),
                        to_mint: Some(u256_to_h256(data.to_mint).as_bytes().into()),
                        refund_recipient_address: Some(data.refund_recipient.as_bytes().into()),
                    },
                )
            }
        };
        let execute = proto::Execute {
            contract_address: Some(this.execute.contract_address.as_bytes().into()),
            calldata: Some(this.execute.calldata.clone()),
            value: Some(u256_to_h256(this.execute.value).as_bytes().into()),
            factory_deps: match &this.execute.factory_deps {
                Some(inner) => inner.clone(),
                None => vec![],
            },
        };
        Self {
            common_data: Some(common_data),
            execute: Some(execute),
            received_timestamp_ms: Some(this.received_timestamp_ms),
            raw_bytes: this.raw_bytes.as_ref().map(|inner| inner.0.clone()),
        }
    }
}

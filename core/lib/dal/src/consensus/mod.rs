pub mod proto;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod testonly;

use anyhow::{anyhow, Context as _};
use zksync_consensus_roles::{attester,validator};
use zksync_protobuf::{read_required, required, ProtoFmt, ProtoRepr};
use zksync_types::{
    abi, ethabi,
    fee::Fee,
    l1::{OpProcessingType, PriorityQueueType},
    l2::TransactionType,
    protocol_upgrade::ProtocolUpgradeTxCommonData,
    transaction_request::PaymasterParams,
    Address, Execute, ExecuteTransactionCommon, InputData, L1BatchNumber, L1TxCommonData,
    L2TxCommonData, Nonce, PriorityOpId, ProtocolVersionId, Transaction, H256,
};
use zksync_utils::{h256_to_u256, u256_to_h256};

use crate::models::{parse_h160, parse_h256};

/// Global attestation status served by
/// `attestationStatus` RPC.
#[derive(Debug,PartialEq,Clone)]
pub struct AttestationStatus {
    pub genesis: validator::GenesisHash,
    pub next_batch_to_attest: attester::BatchNumber,
}

impl ProtoFmt for AttestationStatus {
    type Proto = proto::AttestationStatus;
    
    fn read(r: &Self::Proto) -> anyhow::Result<Self> {
        Ok(Self {
            genesis: read_required(&r.genesis).context("genesis")?,
            next_batch_to_attest: attester::BatchNumber(
                *required(&r.next_batch_to_attest).context("next_batch_to_attest")?,
            ),
        })
    }

    fn build(&self) -> Self::Proto {
        Self::Proto {
            genesis: Some(self.genesis.build()),
            next_batch_to_attest: Some(self.next_batch_to_attest.0),
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
    type Proto = proto::Payload;

    fn read(r: &Self::Proto) -> anyhow::Result<Self> {
        let protocol_version = required(&r.protocol_version)
            .and_then(|x| Ok(ProtocolVersionId::try_from(u16::try_from(*x)?)?))
            .context("protocol_version")?;
        let mut transactions = vec![];

        match protocol_version {
            v if v >= ProtocolVersionId::Version25 => {
                anyhow::ensure!(
                    r.transactions.is_empty(),
                    "transactions should be empty in protocol_version {v}"
                );
                for (i, tx) in r.transactions_v25.iter().enumerate() {
                    transactions.push(
                        tx.read()
                            .with_context(|| format!("transactions_v25[{i}]"))?,
                    );
                }
            }
            v => {
                anyhow::ensure!(
                    r.transactions_v25.is_empty(),
                    "transactions_v25 should be empty in protocol_version {v}"
                );
                for (i, tx) in r.transactions.iter().enumerate() {
                    transactions.push(tx.read().with_context(|| format!("transactions[{i}]"))?)
                }
            }
        }

        Ok(Self {
            protocol_version,
            hash: required(&r.hash)
                .and_then(|h| parse_h256(h))
                .context("hash")?,
            l1_batch_number: L1BatchNumber(
                *required(&r.l1_batch_number).context("l1_batch_number")?,
            ),
            timestamp: *required(&r.timestamp).context("timestamp")?,
            l1_gas_price: *required(&r.l1_gas_price).context("l1_gas_price")?,
            l2_fair_gas_price: *required(&r.l2_fair_gas_price).context("l2_fair_gas_price")?,
            fair_pubdata_price: r.fair_pubdata_price,
            virtual_blocks: *required(&r.virtual_blocks).context("virtual_blocks")?,
            operator_address: required(&r.operator_address)
                .and_then(|a| parse_h160(a))
                .context("operator_address")?,
            transactions,
            last_in_batch: *required(&r.last_in_batch).context("last_in_batch")?,
        })
    }

    fn build(&self) -> Self::Proto {
        let mut x = Self::Proto {
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
            transactions: vec![],
            transactions_v25: vec![],
            last_in_batch: Some(self.last_in_batch),
        };
        match self.protocol_version {
            v if v >= ProtocolVersionId::Version25 => {
                x.transactions_v25 = self.transactions.iter().map(ProtoRepr::build).collect();
            }
            _ => {
                x.transactions = self.transactions.iter().map(ProtoRepr::build).collect();
            }
        }
        x
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

impl ProtoRepr for proto::TransactionV25 {
    type Type = Transaction;

    fn read(&self) -> anyhow::Result<Self::Type> {
        use proto::transaction_v25::T;
        let tx = match required(&self.t)? {
            T::L1(l1) => abi::Transaction::L1 {
                tx: required(&l1.rlp)
                    .and_then(|x| {
                        let tokens = ethabi::decode(&[abi::L2CanonicalTransaction::schema()], x)
                            .context("ethabi::decode()")?;
                        // Unwrap is safe because `ethabi::decode` does the verification.
                        let tx =
                            abi::L2CanonicalTransaction::decode(tokens.into_iter().next().unwrap())
                                .context("L2CanonicalTransaction::decode()")?;
                        Ok(tx)
                    })
                    .context("rlp")?
                    .into(),
                factory_deps: l1.factory_deps.clone(),
                eth_block: 0,
            },
            T::L2(l2) => abi::Transaction::L2(required(&l2.rlp).context("rlp")?.clone()),
        };
        tx.try_into()
    }

    fn build(tx: &Self::Type) -> Self {
        let tx = abi::Transaction::try_from(tx.clone()).unwrap();
        use proto::transaction_v25::T;
        Self {
            t: Some(match tx {
                abi::Transaction::L1 {
                    tx, factory_deps, ..
                } => T::L1(proto::L1Transaction {
                    rlp: Some(ethabi::encode(&[tx.encode()])),
                    factory_deps,
                }),
                abi::Transaction::L2(tx) => T::L2(proto::L2Transaction { rlp: Some(tx) }),
            }),
        }
    }
}

impl ProtoRepr for proto::Transaction {
    type Type = Transaction;

    fn read(&self) -> anyhow::Result<Self::Type> {
        let common_data = required(&self.common_data).context("common_data")?;
        let execute = required(&self.execute).context("execute")?;
        Ok(Self::Type {
            common_data: match common_data {
                proto::transaction::CommonData::L1(common_data) => {
                    anyhow::ensure!(
                        *required(&common_data.deadline_block)
                            .context("common_data.deadline_block")?
                            == 0
                    );
                    anyhow::ensure!(
                        required(&common_data.eth_hash)
                            .and_then(|x| parse_h256(x))
                            .context("common_data.eth_hash")?
                            == H256::default()
                    );
                    ExecuteTransactionCommon::L1(L1TxCommonData {
                        sender: required(&common_data.sender_address)
                            .and_then(|x| parse_h160(x))
                            .context("common_data.sender_address")?,
                        serial_id: required(&common_data.serial_id)
                            .map(|x| PriorityOpId(*x))
                            .context("common_data.serial_id")?,
                        layer_2_tip_fee: required(&common_data.layer_2_tip_fee)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("common_data.layer_2_tip_fee")?,
                        full_fee: required(&common_data.full_fee)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("common_data.full_fee")?,
                        max_fee_per_gas: required(&common_data.max_fee_per_gas)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("common_data.max_fee_per_gas")?,
                        gas_limit: required(&common_data.gas_limit)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("common_data.gas_limit")?,
                        gas_per_pubdata_limit: required(&common_data.gas_per_pubdata_limit)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("common_data.gas_per_pubdata_limit")?,
                        op_processing_type: required(&common_data.op_processing_type)
                            .and_then(|x| {
                                OpProcessingType::try_from(u8::try_from(*x)?)
                                    .map_err(|_| anyhow!("u8::try_from"))
                            })
                            .context("common_data.op_processing_type")?,
                        priority_queue_type: required(&common_data.priority_queue_type)
                            .and_then(|x| {
                                PriorityQueueType::try_from(u8::try_from(*x)?)
                                    .map_err(|_| anyhow!("u8::try_from"))
                            })
                            .context("common_data.priority_queue_type")?,
                        eth_block: *required(&common_data.eth_block)
                            .context("common_data.eth_block")?,
                        canonical_tx_hash: required(&common_data.canonical_tx_hash)
                            .and_then(|x| parse_h256(x))
                            .context("common_data.canonical_tx_hash")?,
                        to_mint: required(&common_data.to_mint)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("common_data.to_mint")?,
                        refund_recipient: required(&common_data.refund_recipient_address)
                            .and_then(|x| parse_h160(x))
                            .context("common_data.refund_recipient_address")?,
                    })
                }
                proto::transaction::CommonData::L2(common_data) => {
                    ExecuteTransactionCommon::L2(L2TxCommonData {
                        nonce: required(&common_data.nonce)
                            .map(|x| Nonce(*x))
                            .context("common_data.nonce")?,
                        fee: Fee {
                            gas_limit: required(&common_data.gas_limit)
                                .and_then(|x| parse_h256(x))
                                .map(h256_to_u256)
                                .context("common_data.gas_limit")?,
                            max_fee_per_gas: required(&common_data.max_fee_per_gas)
                                .and_then(|x| parse_h256(x))
                                .map(h256_to_u256)
                                .context("common_data.max_fee_per_gas")?,
                            max_priority_fee_per_gas: required(
                                &common_data.max_priority_fee_per_gas,
                            )
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("common_data.max_priority_fee_per_gas")?,
                            gas_per_pubdata_limit: required(&common_data.gas_per_pubdata_limit)
                                .and_then(|x| parse_h256(x))
                                .map(h256_to_u256)
                                .context("common_data.gas_per_pubdata_limit")?,
                        },
                        initiator_address: required(&common_data.initiator_address)
                            .and_then(|x| parse_h160(x))
                            .context("common_data.initiator_address")?,
                        signature: required(&common_data.signature)
                            .context("common_data.signature")?
                            .clone(),
                        transaction_type: required(&common_data.transaction_type)
                            .and_then(|x| Ok(TransactionType::try_from(*x)?))
                            .context("common_data.transaction_type")?,
                        input: {
                            match &common_data.input {
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
                            let params = required(&common_data.paymaster_params)?;
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
                proto::transaction::CommonData::ProtocolUpgrade(common_data) => {
                    ExecuteTransactionCommon::ProtocolUpgrade(ProtocolUpgradeTxCommonData {
                        sender: required(&common_data.sender_address)
                            .and_then(|x| parse_h160(x))
                            .context("common_data.sender_address")?,
                        upgrade_id: required(&common_data.upgrade_id)
                            .and_then(|x| Ok(ProtocolVersionId::try_from(u16::try_from(*x)?)?))
                            .context("common_data.upgrade_id")?,
                        max_fee_per_gas: required(&common_data.max_fee_per_gas)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("common_data.max_fee_per_gas")?,
                        gas_limit: required(&common_data.gas_limit)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("common_data.gas_limit")?,
                        gas_per_pubdata_limit: required(&common_data.gas_per_pubdata_limit)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("common_data.gas_per_pubdata_limit")?,
                        eth_block: *required(&common_data.eth_block)
                            .context("common_data.eth_block")?,
                        canonical_tx_hash: required(&common_data.canonical_tx_hash)
                            .and_then(|x| parse_h256(x))
                            .context("common_data.canonical_tx_hash")?,
                        to_mint: required(&common_data.to_mint)
                            .and_then(|x| parse_h256(x))
                            .map(h256_to_u256)
                            .context("common_data.to_mint")?,
                        refund_recipient: required(&common_data.refund_recipient_address)
                            .and_then(|x| parse_h160(x))
                            .context("common_data.refund_recipient_address")?,
                    })
                }
            },
            execute: Execute {
                contract_address: required(&execute.contract_address)
                    .and_then(|x| parse_h160(x))
                    .context("execute.contract_address")?,
                calldata: required(&execute.calldata).context("calldata")?.clone(),
                value: required(&execute.value)
                    .and_then(|x| parse_h256(x))
                    .map(h256_to_u256)
                    .context("execute.value")?,
                factory_deps: execute.factory_deps.clone(),
            },
            received_timestamp_ms: 0, // This timestamp is local to the node
            raw_bytes: self.raw_bytes.as_ref().map(|x| x.clone().into()),
        })
    }

    fn build(this: &Self::Type) -> Self {
        let common_data = match &this.common_data {
            ExecuteTransactionCommon::L1(data) => {
                proto::transaction::CommonData::L1(proto::L1TxCommonData {
                    sender_address: Some(data.sender.as_bytes().into()),
                    serial_id: Some(data.serial_id.0),
                    deadline_block: Some(0),
                    layer_2_tip_fee: Some(u256_to_h256(data.layer_2_tip_fee).as_bytes().into()),
                    full_fee: Some(u256_to_h256(data.full_fee).as_bytes().into()),
                    max_fee_per_gas: Some(u256_to_h256(data.max_fee_per_gas).as_bytes().into()),
                    gas_limit: Some(u256_to_h256(data.gas_limit).as_bytes().into()),
                    gas_per_pubdata_limit: Some(
                        u256_to_h256(data.gas_per_pubdata_limit).as_bytes().into(),
                    ),
                    op_processing_type: Some(data.op_processing_type as u32),
                    priority_queue_type: Some(data.priority_queue_type as u32),
                    eth_hash: Some(H256::default().as_bytes().into()),
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
                        eth_hash: Some(H256::default().as_bytes().into()),
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
            factory_deps: this.execute.factory_deps.clone(),
        };
        Self {
            common_data: Some(common_data),
            execute: Some(execute),
            raw_bytes: this.raw_bytes.as_ref().map(|inner| inner.0.clone()),
        }
    }
}

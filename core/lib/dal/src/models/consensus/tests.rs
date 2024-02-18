use std::fmt::Debug;

use zksync_protobuf::{
    repr::{decode, encode},
    ProtoRepr,
};
use zksync_types::{
    l1::{OpProcessingType, PriorityQueueType},
    protocol_version::ProtocolUpgradeTxCommonData,
    Address, Bytes, Execute, ExecuteTransactionCommon, L1TxCommonData, L2TxCommonData,
    PriorityOpId, ProtocolVersionId, Transaction, H256, U256,
};

use crate::tests::{mock_l1_execute, mock_l2_transaction, mock_protocol_upgrade_transaction};

/// Tests struct <-> proto struct conversions.
#[test]
fn test_encoding() {
    encode_decode::<super::proto::Transaction, ComparableTransaction>(mock_l1_execute().into());
    encode_decode::<super::proto::Transaction, ComparableTransaction>(mock_l2_transaction().into());
    encode_decode::<super::proto::Transaction, ComparableTransaction>(
        mock_protocol_upgrade_transaction().into(),
    );
}

fn encode_decode<P, C>(msg: P::Type)
where
    P: ProtoRepr,
    C: From<P::Type> + PartialEq + Debug,
{
    let got = decode::<P>(&encode::<P>(&msg)).unwrap();
    let (got, msg): (C, C) = (msg.into(), got.into());
    assert_eq!(&msg, &got, "binary encoding");
}

/// Derivative of `Transaction` to facilitate equality comparisons.
#[derive(PartialEq, Debug)]
pub struct ComparableTransaction {
    l1_common_data: Option<ComparableL1TxCommonData>,
    l2_common_data: Option<L2TxCommonData>,
    protocol_upgrade_common_data: Option<ComparableProtocolUpgradeTxCommonData>,
    execute: Execute,
    received_timestamp_ms: u64,
    raw_bytes: Option<Bytes>,
}

impl From<Transaction> for ComparableTransaction {
    fn from(tx: Transaction) -> Self {
        let (l1_common_data, l2_common_data, protocol_upgrade_common_data) = match tx.common_data {
            ExecuteTransactionCommon::L1(data) => (Some(data), None, None),
            ExecuteTransactionCommon::L2(data) => (None, Some(data), None),
            ExecuteTransactionCommon::ProtocolUpgrade(data) => (None, None, Some(data)),
        };
        Self {
            l1_common_data: l1_common_data.map(|x| x.into()),
            l2_common_data,
            protocol_upgrade_common_data: protocol_upgrade_common_data.map(|x| x.into()),
            execute: tx.execute,
            received_timestamp_ms: tx.received_timestamp_ms,
            raw_bytes: tx.raw_bytes,
        }
    }
}

/// Derivative of `L1TxCommonData` to facilitate equality comparisons.
#[derive(PartialEq, Debug)]
pub struct ComparableL1TxCommonData {
    pub sender: Address,
    pub serial_id: PriorityOpId,
    pub deadline_block: u64,
    pub layer_2_tip_fee: U256,
    pub full_fee: U256,
    pub max_fee_per_gas: U256,
    pub gas_limit: U256,
    pub gas_per_pubdata_limit: U256,
    pub op_processing_type: OpProcessingType,
    pub priority_queue_type: PriorityQueueType,
    pub eth_hash: H256,
    pub eth_block: u64,
    pub canonical_tx_hash: H256,
    pub to_mint: U256,
    pub refund_recipient: Address,
}

impl From<L1TxCommonData> for ComparableL1TxCommonData {
    fn from(data: L1TxCommonData) -> Self {
        Self {
            sender: data.sender,
            serial_id: data.serial_id,
            deadline_block: data.deadline_block,
            layer_2_tip_fee: data.layer_2_tip_fee,
            full_fee: data.full_fee,
            max_fee_per_gas: data.max_fee_per_gas,
            gas_limit: data.gas_limit,
            gas_per_pubdata_limit: data.gas_per_pubdata_limit,
            op_processing_type: data.op_processing_type,
            priority_queue_type: data.priority_queue_type,
            eth_hash: data.eth_hash,
            eth_block: data.eth_block,
            canonical_tx_hash: data.canonical_tx_hash,
            to_mint: data.to_mint,
            refund_recipient: data.refund_recipient,
        }
    }
}

/// Derivative of `ProtocolUpgradeTxCommonData` to facilitate equality comparisons.
#[derive(PartialEq, Debug)]
pub struct ComparableProtocolUpgradeTxCommonData {
    pub sender: Address,
    pub upgrade_id: ProtocolVersionId,
    pub max_fee_per_gas: U256,
    pub gas_limit: U256,
    pub gas_per_pubdata_limit: U256,
    pub eth_hash: H256,
    pub eth_block: u64,
    pub canonical_tx_hash: H256,
    pub to_mint: U256,
    pub refund_recipient: Address,
}

impl From<ProtocolUpgradeTxCommonData> for ComparableProtocolUpgradeTxCommonData {
    fn from(data: ProtocolUpgradeTxCommonData) -> Self {
        Self {
            sender: data.sender,
            upgrade_id: data.upgrade_id,
            max_fee_per_gas: data.max_fee_per_gas,
            gas_limit: data.gas_limit,
            gas_per_pubdata_limit: data.gas_per_pubdata_limit,
            eth_hash: data.eth_hash,
            eth_block: data.eth_block,
            canonical_tx_hash: data.canonical_tx_hash,
            to_mint: data.to_mint,
            refund_recipient: data.refund_recipient,
        }
    }
}

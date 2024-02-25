use std::fmt::Debug;

use zksync_protobuf::{
    repr::{decode, encode},
    ProtoRepr,
};
use zksync_types::{Bytes, Execute, ExecuteTransactionCommon, Transaction};

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
    common_data: ExecuteTransactionCommon,
    execute: Execute,
    received_timestamp_ms: u64,
    raw_bytes: Option<Bytes>,
}

impl From<Transaction> for ComparableTransaction {
    fn from(tx: Transaction) -> Self {
        Self {
            common_data: tx.common_data,
            execute: tx.execute,
            received_timestamp_ms: tx.received_timestamp_ms,
            raw_bytes: tx.raw_bytes,
        }
    }
}

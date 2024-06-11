use std::fmt::Debug;

use zksync_protobuf::{
    repr::{decode, encode},
    ProtoRepr,
};
use zksync_types::{web3::Bytes, Execute, ExecuteTransactionCommon, Transaction};

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
    assert_eq!(&C::from(msg), &C::from(got), "binary encoding");
}

/// Derivative of `Transaction` to facilitate equality comparisons.
#[derive(PartialEq, Debug)]
pub struct ComparableTransaction {
    common_data: ExecuteTransactionCommon,
    execute: Execute,
    raw_bytes: Option<Bytes>,
    // `received_timestamp_ms` is intentionally not included because it's local
}

impl From<Transaction> for ComparableTransaction {
    fn from(tx: Transaction) -> Self {
        Self {
            common_data: tx.common_data,
            execute: tx.execute,
            raw_bytes: tx.raw_bytes,
        }
    }
}

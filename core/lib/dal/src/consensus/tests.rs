use std::fmt::Debug;

use rand::Rng;
use zksync_concurrency::ctx;
use zksync_protobuf::{
    repr::{decode, encode},
    testonly::{test_encode, test_encode_random},
    ProtoRepr,
};
use zksync_test_account::Account;
use zksync_types::{
    web3::Bytes, Execute, ExecuteTransactionCommon, L1BatchNumber, ProtocolVersionId, Transaction,
};

use super::{proto, AttestationStatus, Payload};
use crate::tests::mock_protocol_upgrade_transaction;

fn execute(rng: &mut impl Rng) -> Execute {
    Execute {
        contract_address: rng.gen(),
        value: rng.gen::<u128>().into(),
        calldata: (0..10 * 32).map(|_| rng.gen()).collect(),
        // TODO: find a way to generate valid random bytecode.
        factory_deps: vec![],
    }
}

fn l1_transaction(rng: &mut impl Rng) -> Transaction {
    Account::random_using(rng).get_l1_tx(execute(rng), rng.gen())
}

fn l2_transaction(rng: &mut impl Rng) -> Transaction {
    Account::random_using(rng).get_l2_tx_for_execute(execute(rng), None)
}

fn payload(rng: &mut impl Rng, protocol_version: ProtocolVersionId) -> Payload {
    Payload {
        protocol_version,
        hash: rng.gen(),
        l1_batch_number: L1BatchNumber(rng.gen()),
        timestamp: rng.gen(),
        l1_gas_price: rng.gen(),
        l2_fair_gas_price: rng.gen(),
        fair_pubdata_price: Some(rng.gen()),
        virtual_blocks: rng.gen(),
        operator_address: rng.gen(),
        transactions: (0..10)
            .map(|_| match rng.gen() {
                true => l1_transaction(rng),
                false => l2_transaction(rng),
            })
            .collect(),
        last_in_batch: rng.gen(),
    }
}

/// Tests struct <-> proto struct conversions.
#[test]
fn test_encoding() {
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();
    test_encode_random::<AttestationStatus>(rng);
    encode_decode::<proto::TransactionV25, ComparableTransaction>(l1_transaction(rng));
    encode_decode::<proto::TransactionV25, ComparableTransaction>(l2_transaction(rng));
    encode_decode::<proto::Transaction, ComparableTransaction>(l1_transaction(rng));
    encode_decode::<proto::Transaction, ComparableTransaction>(l2_transaction(rng));
    encode_decode::<proto::Transaction, ComparableTransaction>(
        mock_protocol_upgrade_transaction().into(),
    );
    let p = payload(rng, ProtocolVersionId::Version24);
    test_encode(rng, &p);
    let p = payload(rng, ProtocolVersionId::Version25);
    test_encode(rng, &p);
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

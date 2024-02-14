use zksync_protobuf_config::repr::ProtoRepr;

use crate::tests::{mock_l1_execute, mock_l2_transaction};

/// Tests struct <-> proto struct conversions.
#[test]
fn test_encoding() {
    encode_decode::<super::proto::Transaction>(mock_l2_transaction().into());
    encode_decode::<super::proto::Transaction>(mock_l1_execute().into());
}

fn encode_decode<P: ProtoRepr>(msg: P::Type)
where
    P::Type: PartialEq + std::fmt::Debug,
{
    let got = decode::<P>(&encode::<P>(&msg)).unwrap();
    assert_eq!(&msg, &got, "binary encoding");
}

fn encode<P: ProtoRepr>(msg: &P::Type) -> Vec<u8> {
    let msg = P::build(msg);
    zksync_protobuf::canonical_raw(&msg.encode_to_vec(), &msg.descriptor()).unwrap()
}

fn decode<P: ProtoRepr>(bytes: &[u8]) -> anyhow::Result<P::Type> {
    P::read(&P::decode(bytes)?)
}

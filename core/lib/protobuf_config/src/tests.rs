use super::ProtoRepr;
use zksync_basic_types::H256;

fn encode<P: ProtoRepr>(msg: &P::Type) -> Vec<u8> {
    let msg = P::build(msg);
    zksync_protobuf::canonical_raw(&msg.encode_to_vec(), &msg.descriptor()).unwrap()
}

fn decode<P: ProtoRepr>(bytes: &[u8]) -> anyhow::Result<P::Type> {
    P::read(&P::decode(bytes)?)
}

fn encode_json<P: ProtoRepr>(msg: &P::Type) -> String {
    zksync_protobuf::encode_json_proto(&P::build(msg))
}

fn decode_json<P: ProtoRepr>(json: &str) -> anyhow::Result<P::Type> {
    P::read(&zksync_protobuf::decode_json_proto(json)?)
}

#[test]
fn test_encoding() {
    use super::proto::Web3JsonRpc as Proto;
    // TODO(gprusak): this test is hardly representative because many fields are not set.
    // Use `zksync_protobuf::testonly::test_encoding_random()` instead.
    let mut want = super::Web3JsonRpcConfig::for_tests();
    want.account_pks = Some(vec![H256::repeat_byte(1), H256::repeat_byte(2)]);
    let got: super::Web3JsonRpcConfig = decode::<Proto>(&encode::<Proto>(&want)).unwrap();
    assert_eq!(&want, &got);
    let got: super::Web3JsonRpcConfig = decode_json::<Proto>(&encode_json::<Proto>(&want)).unwrap();
    assert_eq!(&want, &got);
    println!("{}", encode_json::<Proto>(&want));
}

use zksync_basic_types::H256;
use zksync_protobuf::{decode, decode_json, encode, encode_json};

#[test]
fn test_encoding() {
    // TODO(gprusak): this test is hardly representative because many fields are not set.
    // Use `zksync_protobuf::testonly::test_encoding_random()` instead.
    let mut want = super::api::Web3JsonRpcConfig::for_tests();
    want.account_pks = Some(vec![H256::repeat_byte(1), H256::repeat_byte(2)]);
    let got: super::api::Web3JsonRpcConfig = decode(&encode(&want)).unwrap();
    assert_eq!(&want, &got);
    let got: super::api::Web3JsonRpcConfig = decode_json(&encode_json(&want)).unwrap();
    assert_eq!(&want, &got);
    println!("{}", encode_json(&want));
}

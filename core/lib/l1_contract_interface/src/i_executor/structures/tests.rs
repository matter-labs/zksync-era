use rand::{
    distributions::{Distribution, Standard},
    Rng,
};

use super::*;

impl Distribution<StoredBatchInfo> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> StoredBatchInfo {
        StoredBatchInfo {
            batch_number: rng.gen(),
            batch_hash: rng.gen(),
            index_repeated_storage_changes: rng.gen(),
            number_of_layer1_txs: rng.gen::<u64>().into(),
            priority_operations_hash: rng.gen(),
            l2_logs_tree_root: rng.gen(),
            timestamp: rng.gen::<u64>().into(),
            commitment: rng.gen(),
            airbender_commitment: rng.gen(),
            dependency_roots_rolling_hash: rng.gen(),
        }
    }
}

/// Test checking encoding and decoding of `StoredBatchInfo`.
#[test]
fn test_encoding() {
    let rng = &mut rand::thread_rng();
    for _ in 0..10 {
        let mut want: StoredBatchInfo = rng.gen();
        want.airbender_commitment = zksync_types::H256::zero();
        let got = StoredBatchInfo::decode(
            &want.encode(ProtocolVersionId::latest()),
            ProtocolVersionId::latest(),
        )
        .unwrap();
        assert_eq!(want, got);
    }
}

#[test]
fn stored_batch_schema_versions() {
    use zksync_types::{
        ethabi::{encode, Token},
        H256,
    };
    let batch: StoredBatchInfo = rand::random();
    for (version, length) in [
        (ProtocolVersionId::Version26, 8),
        (ProtocolVersionId::Version31, 9),
        (ProtocolVersionId::Version32, 9),
        (ProtocolVersionId::Version33, 9),
        (ProtocolVersionId::Version34, 10),
        (ProtocolVersionId::Version35, 10),
    ] {
        let token = batch.clone().into_token_with_protocol_version(version);
        let Token::Tuple(fields) = &token else {
            unreachable!()
        };
        assert_eq!(fields.len(), length);
        let encoded = encode(&[token]);
        assert_eq!(batch.encode(version), encoded);
        assert_eq!(
            batch.hash(version),
            H256(zksync_types::web3::keccak256(&encoded))
        );
        let decoded = StoredBatchInfo::decode(&encoded, version).unwrap();
        let mut expected = batch.clone();
        if length < 10 {
            expected.airbender_commitment = H256::zero();
        }
        if length < 9 {
            expected.dependency_roots_rolling_hash = H256::zero();
        }
        assert_eq!(decoded, expected);
    }
    assert_eq!(get_encoding_version(ProtocolVersionId::Version31), 1);
    assert_eq!(get_encoding_version(ProtocolVersionId::Version32), 1);
    assert_eq!(get_encoding_version(ProtocolVersionId::Version33), 1);
    assert_eq!(get_encoding_version(ProtocolVersionId::Version34), 5);
    assert_eq!(get_encoding_version(ProtocolVersionId::Version35), 5);
}

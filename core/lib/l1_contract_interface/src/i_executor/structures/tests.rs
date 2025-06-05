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
            dependency_roots_rolling_hash: rng.gen(),
            l2_logs_tree_root: rng.gen(),
            timestamp: rng.gen::<u64>().into(),
            commitment: rng.gen(),
        }
    }
}

/// Test checking encoding and decoding of `StoredBatchInfo`.
#[test]
fn test_encoding() {
    let rng = &mut rand::thread_rng();
    for _ in 0..10 {
        let want: StoredBatchInfo = rng.gen();
        let got = StoredBatchInfo::decode(&want.encode()).unwrap();
        assert_eq!(want, got);
    }
}

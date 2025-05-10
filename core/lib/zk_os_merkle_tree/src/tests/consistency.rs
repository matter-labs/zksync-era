use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};
use tempfile::TempDir;
use zksync_basic_types::H256;

use crate::{MerkleTree, MerkleTreeColumnFamily, RocksDBWrapper, TreeEntry};

// Something (maybe RocksDB) makes the test below work very slowly in the debug mode;
// thus, the number of test cases is conditionally reduced.
const ITER_COUNT: usize = if cfg!(debug_assertions) { 10 } else { 5_000 };

/// Tests that if a single key is removed from the DB, or a single bit is changed in a value,
/// the tree does not pass a consistency check.
#[test]
fn five_thousand_angry_monkeys_vs_merkle_tree() {
    const RNG_SEED: u64 = 42;

    let dir = TempDir::new().expect("failed creating temporary dir for RocksDB");
    let mut db = RocksDBWrapper::new(dir.path()).unwrap();
    let mut tree = MerkleTree::new(&mut db).unwrap();
    let mut rng = StdRng::seed_from_u64(RNG_SEED);

    let entries: Vec<_> = (1..=100)
        .map(|_| TreeEntry {
            key: H256(rng.gen()),
            value: H256(rng.gen()),
        })
        .collect();
    tree.extend(&entries).unwrap();
    tree.verify_consistency(0).unwrap();

    let mut raw_db = db.into_inner();
    let cf = MerkleTreeColumnFamily::Tree;
    // Load all key-node pairs from the 0-th version of the tree.
    let raw_kvs: Vec<_> = raw_db.prefix_iterator_cf(cf, &[0; 8]).collect();
    assert!(raw_kvs.len() > 100);

    for _ in 0..ITER_COUNT {
        let (key, value) = raw_kvs.choose(&mut rng).unwrap();
        let should_remove = rng.gen();

        let mut batch = raw_db.new_write_batch();
        if should_remove {
            println!("deleting value at {key:?}");
            batch.delete_cf(cf, key);
        } else {
            let mut mangled_value = value.to_vec();
            let mangled_idx = rng.gen_range(0..mangled_value.len());
            let mangled_bit = rng.gen_range(0..8);
            mangled_value[mangled_idx] ^= 1 << mangled_bit;
            println!("mangling bit {mangled_idx}:{mangled_bit} of the value at {key:?}");
            batch.put_cf(cf, key, &mangled_value);
        }
        raw_db.write(batch).unwrap();

        let mut db = RocksDBWrapper::from(raw_db);
        let err = MerkleTree::new(&mut db)
            .unwrap()
            .verify_consistency(0)
            .unwrap_err();
        println!("{err}");

        // Restore the value back so that it doesn't influence the following cases.
        raw_db = db.into_inner();
        let mut reverse_batch = raw_db.new_write_batch();
        reverse_batch.put_cf(cf, key, value);
        raw_db.write(reverse_batch).unwrap();
    }
}

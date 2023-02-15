//! Module used to calculate root hashes for small in-memory merkle trees
//!
//! Picks height for the tree and builds merkle tree on a fly from vector of values.
//! Latter will be used as tree leaves.
//! Resulted tree is left-leaning, meaning that all meaningful non-trivial paths will be stored in the left subtree.
use once_cell::sync::OnceCell;
use rayon::prelude::{IntoParallelIterator, ParallelIterator, ParallelSlice};
use std::cmp::max;
use std::collections::HashMap;
use zksync_basic_types::H256;
use zksync_crypto::hasher::keccak::KeccakHasher;
use zksync_crypto::hasher::Hasher;

const MINI_TREE_MIN_DEPTH: usize = 5;
const MINI_TREE_MAX_DEPTH: usize = 10;
const MAX_NUMBER_OF_LEAVES: u32 = 2_u32.pow(MINI_TREE_MAX_DEPTH as u32);
type ZkHasher = KeccakHasher;
type EmptyTree = Vec<Vec<u8>>;

/// Computes root hash of merkle tree by given list of leaf values. The leaves could have different sizes, normally
/// it's 32 bytes. But it's not always an option, e.g. leaf for L2L1Message is 88 bytes.
pub fn mini_merkle_tree_root_hash<V>(values: Vec<V>, leaf_size: usize, tree_size: usize) -> H256
where
    V: IntoIterator<Item = u8> + Send,
{
    if values.is_empty() {
        return H256::zero();
    }
    H256::from_slice(
        mini_merkle_tree_proof(values, 0, leaf_size, tree_size)
            .pop()
            .unwrap()
            .as_slice(),
    )
}

/// Recalculates hashes in merkle tree and returns root hash and merkle proof for specified leaf
pub fn mini_merkle_tree_proof<V>(
    values: Vec<V>,
    mut idx: usize,
    leaf_size: usize,
    tree_size: usize,
) -> Vec<Vec<u8>>
where
    V: IntoIterator<Item = u8> + Send,
{
    assert!(idx < values.len(), "invalid tree leaf index");
    assert!(
        values.len() as u32 <= MAX_NUMBER_OF_LEAVES,
        "number of leaves exceeds merkle tree capacity"
    );

    // pick merkle tree depth
    let depth = max(tree_size.trailing_zeros() as usize, MINI_TREE_MIN_DEPTH);
    let empty_tree = empty_tree(depth, leaf_size);

    // compute leaf hashes
    let hasher = ZkHasher::default();
    let mut current_level: Vec<_> = values
        .into_par_iter()
        .map(|value| hasher.hash_bytes(value))
        .collect();

    // iterate tree level by level bottom-up, group neighbour nodes and emit their cumulative hash
    let mut proof = Vec::with_capacity(depth + 1);
    for level_idx in 1..=depth {
        let default_value = empty_tree[level_idx - 1].clone();
        let neighbour_idx = idx ^ 1;
        let neighbour_hash = current_level.get(neighbour_idx).unwrap_or(&default_value);
        proof.push(neighbour_hash.clone());

        current_level = current_level
            .par_chunks(2)
            .map(|chunk| {
                let right = chunk.get(1).unwrap_or(&default_value);
                hasher.compress(&chunk[0], right)
            })
            .collect();
        idx /= 2;
    }
    proof.push(current_level[0].clone());
    proof
}

/// Empty tree hashes for mini merkle tree of specified depth.
/// Uses `once_cell` internally, thus hashes must be precalculated only once.
fn empty_tree(depth: usize, leaf_size: usize) -> &'static EmptyTree {
    assert!(
        (MINI_TREE_MIN_DEPTH..=MINI_TREE_MAX_DEPTH).contains(&depth),
        "merkle tree depth is out of range"
    );
    static CONFIGS: OnceCell<HashMap<usize, EmptyTree>> = OnceCell::new();
    &CONFIGS.get_or_init(|| {
        let hasher = ZkHasher::default();
        (MINI_TREE_MIN_DEPTH..=MINI_TREE_MAX_DEPTH)
            .map(|depth| {
                let mut hashes = Vec::with_capacity(depth + 1);
                hashes.push(hasher.hash_bytes(vec![0; leaf_size]));

                for _ in 0..depth {
                    let last_hash = hashes.last().unwrap();
                    let hash = hasher.compress(last_hash, last_hash);
                    hashes.push(hash);
                }
                (depth, hashes)
            })
            .collect()
    })[&depth]
}

#[cfg(test)]
mod tests {
    use super::*;

    // #[test]
    // fn calculate_root_hash() {
    //     // trivial tree hash matches default config value
    //     let default_root_hash = empty_tree(MINI_TREE_MIN_DEPTH, 32).last().unwrap().clone();
    //     let values = vec![[0; 32]];
    //     let hash = mini_merkle_tree_root_hash(values, 32).as_bytes().to_vec();
    //     assert_eq!(hash, default_root_hash);
    //
    //     // array of integers
    //     let hash = mini_merkle_tree_root_hash(gen_test_data(1, 8), 32);
    //     assert_eq!(
    //         hash.as_bytes(),
    //         &[
    //             124, 121, 200, 65, 252, 60, 136, 184, 140, 160, 31, 140, 95, 161, 133, 79, 66, 216,
    //             126, 228, 153, 182, 82, 213, 39, 38, 70, 28, 193, 7, 206, 64
    //         ],
    //     );
    //
    //     // large array
    //     let hash = mini_merkle_tree_root_hash(gen_test_data(0, 1000), 32);
    //     assert_eq!(
    //         hash.as_bytes(),
    //         &[
    //             143, 68, 29, 29, 247, 41, 93, 101, 30, 223, 112, 103, 30, 157, 200, 128, 183, 193,
    //             178, 186, 163, 228, 60, 196, 42, 44, 97, 174, 75, 82, 224, 187
    //         ],
    //     );
    // }
    //
    // #[test]
    // fn calculate_merkle_proof() {
    //     let hasher = ZkHasher::default();
    //     let values = gen_test_data(0, 6);
    //     let proof = mini_merkle_tree_proof(values.clone(), 3, 32);
    //     assert_eq!(proof.len(), MINI_TREE_MIN_DEPTH + 1);
    //     assert_eq!(
    //         proof,
    //         vec![
    //             hasher.hash_bytes(vec![2, 0, 0, 0]), // neighbour hash
    //             hasher.compress(
    //                 &hasher.hash_bytes(vec![0; 4]),
    //                 &hasher.hash_bytes(vec![1, 0, 0, 0])
    //             ), // neighbour branch
    //             [
    //                 138, 204, 157, 41, 179, 91, 233, 147, 240, 27, 150, 54, 120, 138, 71, 109, 120,
    //                 53, 187, 156, 232, 131, 65, 96, 227, 224, 157, 108, 15, 150, 30, 123
    //             ]
    //             .to_vec(),
    //             [
    //                 91, 130, 182, 149, 167, 172, 38, 104, 225, 136, 183, 95, 125, 79, 167, 159,
    //                 170, 80, 65, 23, 209, 253, 252, 190, 138, 70, 145, 92, 26, 138, 81, 145
    //             ]
    //             .to_vec(),
    //             mini_merkle_tree_root_hash(values, 32).as_bytes().to_vec(), // root hash
    //         ]
    //     );
    //
    //     // verify merkle tree heights
    //     let proof = mini_merkle_tree_proof(gen_test_data(0, 1000), 0, 32);
    //     assert_eq!(proof.len(), MINI_TREE_MAX_DEPTH + 1);
    //     let proof = mini_merkle_tree_proof(gen_test_data(0, 100), 0, 32);
    //     assert_eq!(proof.len(), 8);
    //     let proof = mini_merkle_tree_proof(gen_test_data(0, 256), 0, 32);
    //     assert_eq!(proof.len(), 9);
    // }

    #[test]
    #[should_panic]
    fn empty_tree_proof() {
        mini_merkle_tree_proof(gen_test_data(0, 0), 0, 32, MINI_TREE_MIN_DEPTH);
    }

    #[test]
    #[should_panic]
    fn invalid_index_fails() {
        mini_merkle_tree_proof(gen_test_data(1, 4), 5, 32, MINI_TREE_MIN_DEPTH);
    }

    #[test]
    #[should_panic]
    fn check_capacity() {
        mini_merkle_tree_root_hash(
            gen_test_data(0, 2 * MAX_NUMBER_OF_LEAVES as usize),
            32,
            MINI_TREE_MIN_DEPTH,
        );
    }

    fn gen_test_data(left: usize, right: usize) -> Vec<Vec<u8>> {
        (left..right)
            .map(|x| vec![(x % 256) as u8, (x / 256) as u8, 0, 0])
            .collect()
    }
}

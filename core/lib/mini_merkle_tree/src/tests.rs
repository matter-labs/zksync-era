//! Tests for `MiniMerkleTree`.

use super::*;

#[test]
fn tree_depth_is_computed_correctly() {
    const TREE_SIZES_AND_DEPTHS: &[(usize, usize)] = &[
        (1, 0),
        (2, 1),
        (4, 2),
        (8, 3),
        (16, 4),
        (32, 5),
        (64, 6),
        (128, 7),
    ];

    for &(size, depth) in TREE_SIZES_AND_DEPTHS {
        println!("tree_depth_by_size({size})");
        assert_eq!(tree_depth_by_size(size), depth);
    }
}

#[test]
fn hash_of_empty_tree_with_single_item() {
    for depth in 0..=5 {
        let len = 1 << depth;
        println!("checking tree with {len} items");
        let tree = MiniMerkleTree::new(iter::once([0_u8; 88]), Some(len));
        assert_eq!(tree.merkle_root(), KeccakHasher.empty_subtree_hash(depth));
    }
}

#[test]
fn hash_of_large_empty_tree_with_multiple_items() {
    for len in [50, 64, 100, 128, 256, 512, 1_000, 1_024] {
        println!("checking tree with {len} items");
        let leaves = iter::repeat([0_u8; 88]).take(len);
        let tree_size = len.next_power_of_two();

        let tree = MiniMerkleTree::new(leaves.clone(), Some(tree_size));
        let depth = tree_depth_by_size(tree_size);
        assert_eq!(tree.merkle_root(), KeccakHasher.empty_subtree_hash(depth));
        let tree = MiniMerkleTree::new(leaves, None);
        let depth = tree_depth_by_size(tree_size);
        assert_eq!(tree.merkle_root(), KeccakHasher.empty_subtree_hash(depth));
    }
}

#[test]
fn single_item_tree_snapshot() {
    let tree = MiniMerkleTree::new(iter::once([1_u8; 88]), Some(32));
    let (root_hash, path) = tree.merkle_root_and_path(0);

    let expected_root_hash: H256 =
        "0x45e2793110bf02d81fc72bc11ed257c79251816f4187fa95f2e4d7cded1efad9"
            .parse()
            .unwrap();
    assert_eq!(root_hash, expected_root_hash);

    let expected_path = [
        "0x72abee45b59e344af8a6e520241c4744aff26ed411f4c4b00f8af09adada43ba",
        "0xc3d03eebfd83049991ea3d3e358b6712e7aa2e2e63dc2d4b438987cec28ac8d0",
        "0xe3697c7f33c31a9b0f0aeb8542287d0d21e8c4cf82163d0c44c7a98aa11aa111",
        "0x199cc5812543ddceeddd0fc82807646a4899444240db2c0d2f20c3cceb5f51fa",
        "0xe4733f281f18ba3ea8775dd62d2fcd84011c8c938f16ea5790fd29a03bf8db89",
    ]
    .map(|s| s.parse::<H256>().unwrap());
    assert_eq!(path, expected_path);
}

#[test]
fn full_tree_snapshot() {
    let leaves = (1_u8..=32).map(|byte| [byte; 88]);
    let tree = MiniMerkleTree::new(leaves, None);
    let (root_hash, path) = tree.merkle_root_and_path(2);

    let expected_root_hash: H256 =
        "0x29694afc5d76ad6ee48e9382b1cf724c503c5742aa905700e290845c56d1b488"
            .parse()
            .unwrap();
    assert_eq!(root_hash, expected_root_hash);

    let expected_path = [
        "0x14c175b8872f2d5fdc604a7815e3db795e1922e6ba3665a252e0039af266761b",
        "0xfa962a43efca9e02922feaac4d164933e4e1eba2019944698cc46f793c2a8733",
        "0xc0ae660cf922e2bff20d24a77f31e5580fbdc28201b496bae5eb1803bf7e96c4",
        "0x7f4f800d9522a26ed963ea2246d34d96eaa0f336272c33d96aaceb2ad8930a48",
        "0x149d20e7d6506f8ce6379b42d4188789564e62127b60f3213d598fb5daeb8f71",
    ]
    .map(|s| s.parse::<H256>().unwrap());
    assert_eq!(path, expected_path);
}

#[test]
fn partial_tree_snapshot() {
    let leaves = (1_u8..=50).map(|byte| [byte; 88]);
    let tree = MiniMerkleTree::new(leaves.clone(), None);
    let (root_hash, path) = tree.merkle_root_and_path(10);

    let expected_root_hash: H256 =
        "0x2da23c4270b612710106f3e02e9db9fa42663751869f48d952fa7a0eaaa92475"
            .parse()
            .unwrap();
    assert_eq!(root_hash, expected_root_hash);

    let expected_path = [
        "0xd2f9d641c8456399c31f33bc0a0bfcce4afca53fa1938f309943b396b26a0199",
        "0xedc4ba9f1c4c28c60293177da454580680d3affc1f10e2ce6248612e66c2f214",
        "0x792b07f0a0e4a0e59fb01767855a90417b4ab8d7beba12619925aa61ec7f02dd",
        "0xca0753b052ffcfec8241499af88d7983e2e30a5ecf2754d5147f560e8cda9a58",
        "0x149d20e7d6506f8ce6379b42d4188789564e62127b60f3213d598fb5daeb8f71",
        "0xc12232b0d6558a3672bc22d484f29f5584683c002df73c9389e83ec929dfed3a",
    ]
    .map(|s| s.parse::<H256>().unwrap());
    assert_eq!(path, expected_path);

    let tree = MiniMerkleTree::new(leaves, None);
    let (root_hash, path) = tree.merkle_root_and_path(49);

    assert_eq!(root_hash, expected_root_hash);

    let expected_path = [
        "0x39f19437665159060317aab8b417352df18779f50b68a6bf6bc9c94dff8c98ca",
        "0xc3d03eebfd83049991ea3d3e358b6712e7aa2e2e63dc2d4b438987cec28ac8d0",
        "0xe3697c7f33c31a9b0f0aeb8542287d0d21e8c4cf82163d0c44c7a98aa11aa111",
        "0x199cc5812543ddceeddd0fc82807646a4899444240db2c0d2f20c3cceb5f51fa",
        "0x6edd774c0492cb4c825e4684330fd1c3259866606d47241ebf2a29af0190b5b1",
        "0x29694afc5d76ad6ee48e9382b1cf724c503c5742aa905700e290845c56d1b488",
    ]
    .map(|s| s.parse::<H256>().unwrap());
    assert_eq!(path, expected_path);
}

fn verify_merkle_proof(
    item: &[u8],
    mut index: usize,
    item_count: usize,
    merkle_path: &[H256],
    merkle_root: H256,
) {
    assert!(index < item_count);
    let tree_depth = tree_depth_by_size(item_count.next_power_of_two());
    assert_eq!(merkle_path.len(), tree_depth);

    let mut hash = KeccakHasher.hash_bytes(item);
    for path_item in merkle_path {
        let (lhs, rhs) = if index % 2 == 0 {
            (&hash, path_item)
        } else {
            (path_item, &hash)
        };
        hash = KeccakHasher.compress(lhs, rhs);
        index /= 2;
    }
    assert_eq!(hash, merkle_root);
}

#[test]
fn merkle_proofs_are_valid_in_small_tree() {
    let leaves = (1_u8..=50).map(|byte| [byte; 88]);
    let tree = MiniMerkleTree::new(leaves.clone(), None);

    for (i, item) in leaves.enumerate() {
        let (merkle_root, path) = tree.clone().merkle_root_and_path(i);
        verify_merkle_proof(&item, i, 50, &path, merkle_root);
    }
}

#[test]
fn merkle_proofs_are_valid_in_larger_tree() {
    let leaves = (1_u8..=255).map(|byte| [byte; 88]);
    let tree = MiniMerkleTree::new(leaves.clone(), Some(512));

    for (i, item) in leaves.enumerate() {
        let (merkle_root, path) = tree.clone().merkle_root_and_path(i);
        verify_merkle_proof(&item, i, 512, &path, merkle_root);
    }
}

#[test]
#[allow(clippy::cast_possible_truncation)] // truncation is intentional
fn merkle_proofs_are_valid_in_very_large_tree() {
    let leaves = (1_u32..=15_000).map(|byte| [byte as u8; 88]);

    let tree = MiniMerkleTree::new(leaves.clone(), None);
    for (i, item) in leaves.clone().enumerate().step_by(61) {
        let (merkle_root, path) = tree.clone().merkle_root_and_path(i);
        verify_merkle_proof(&item, i, 1 << 14, &path, merkle_root);
    }

    let tree_with_min_size = MiniMerkleTree::new(leaves.clone(), Some(512));
    assert_eq!(tree_with_min_size.clone().merkle_root(), tree.merkle_root());
    for (i, item) in leaves.enumerate().step_by(61) {
        let (merkle_root, path) = tree_with_min_size.clone().merkle_root_and_path(i);
        verify_merkle_proof(&item, i, 1 << 14, &path, merkle_root);
    }
}

#[test]
fn merkle_proofs_are_valid_in_very_small_trees() {
    for item_count in 1..=20 {
        let leaves = (1..=item_count).map(|byte| [byte; 88]);

        let tree = MiniMerkleTree::new(leaves.clone(), None);
        let item_count = usize::from(item_count).next_power_of_two();
        for (i, item) in leaves.clone().enumerate() {
            let (merkle_root, path) = tree.clone().merkle_root_and_path(i);
            verify_merkle_proof(&item, i, item_count, &path, merkle_root);
        }

        let tree_with_min_size = MiniMerkleTree::new(leaves.clone(), Some(512));
        assert_ne!(tree_with_min_size.clone().merkle_root(), tree.merkle_root());
        for (i, item) in leaves.enumerate() {
            let (merkle_root, path) = tree_with_min_size.clone().merkle_root_and_path(i);
            verify_merkle_proof(&item, i, 512, &path, merkle_root);
        }
    }
}

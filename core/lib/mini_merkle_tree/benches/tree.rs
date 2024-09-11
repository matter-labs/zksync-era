//! Basic benchmarks for `MiniMerkleTree`.

use criterion::{criterion_group, criterion_main, Bencher, BenchmarkId, Criterion, Throughput};
use zksync_mini_merkle_tree::MiniMerkleTree;

const TREE_SIZES: &[usize] = &[32, 64, 128, 256, 512, 1_024];

fn compute_merkle_root(bencher: &mut Bencher<'_>, tree_size: usize) {
    let leaves = (0..tree_size).map(|i| [i as u8; 88]);
    let tree = MiniMerkleTree::new(leaves, None);
    bencher.iter(|| tree.merkle_root());
}

fn compute_merkle_path(bencher: &mut Bencher<'_>, tree_size: usize) {
    let leaves = (0..tree_size).map(|i| [i as u8; 88]);
    let tree = MiniMerkleTree::new(leaves, None);
    bencher.iter(|| tree.merkle_root_and_path(tree_size / 3));
}

fn basic_benches(criterion: &mut Criterion) {
    let mut merkle_root_benches = criterion.benchmark_group("merkle_root");
    for &tree_size in TREE_SIZES {
        merkle_root_benches
            .bench_with_input(
                BenchmarkId::new("tree_size", tree_size),
                &tree_size,
                |bencher, &tree_size| compute_merkle_root(bencher, tree_size),
            )
            .throughput(Throughput::Elements(tree_size as u64));
    }
    merkle_root_benches.finish();

    let mut merkle_path_benches = criterion.benchmark_group("merkle_path");
    for &tree_size in TREE_SIZES {
        merkle_path_benches
            .bench_with_input(
                BenchmarkId::new("tree_size", tree_size),
                &tree_size,
                |bencher, &tree_size| compute_merkle_path(bencher, tree_size),
            )
            .throughput(Throughput::Elements(tree_size as u64));
    }
    merkle_path_benches.finish();
}

criterion_group!(benches, basic_benches);
criterion_main!(benches);

use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use zksync_vm_benchmark_harness::{
    cut_to_allowed_bytecode_size, get_deploy_tx_with_gas_limit, BenchmarkingVm,
};

const GAS_LIMIT: u32 = 30_000_000;
const TXS_IN_BATCH: &[usize] = &[1, 10, 50, 100, 200, 500, 1_000, 2_000, 5_000];

fn bench_fill_bootloader(c: &mut Criterion) {
    let mut group = c.benchmark_group("fill_bootloader");
    group
        .sample_size(10)
        .measurement_time(Duration::from_secs(10));

    let test_contract =
        std::fs::read("deployment_benchmarks/deploy_simple_contract").expect("failed to read file");
    let code = cut_to_allowed_bytecode_size(&test_contract).unwrap();
    let max_txs = *TXS_IN_BATCH.last().unwrap() as u32;
    let txs: Vec<_> = (0..max_txs)
        .map(|nonce| get_deploy_tx_with_gas_limit(code, GAS_LIMIT, nonce))
        .collect();

    for txs_in_batch in TXS_IN_BATCH {
        group.throughput(Throughput::Elements(*txs_in_batch as u64));
        group.bench_with_input(
            BenchmarkId::new("simple_contract", txs_in_batch),
            txs_in_batch,
            |bencher, &txs_in_batch| {
                bencher.iter(|| {
                    let mut vm = BenchmarkingVm::new();
                    for (i, tx) in txs[..txs_in_batch].iter().enumerate() {
                        let result = vm.run_transaction(black_box(tx)).result;
                        assert!(!result.is_failed(), "{result:?} on tx #{i}");
                    }
                })
            },
        );
    }
}

criterion_group!(benches, bench_fill_bootloader);
criterion_main!(benches);

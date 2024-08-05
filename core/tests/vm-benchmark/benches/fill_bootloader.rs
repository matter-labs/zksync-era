use std::time::Duration;

use criterion::{
    black_box, criterion_group, criterion_main, measurement::WallTime, BenchmarkGroup, BenchmarkId,
    Criterion, Throughput,
};
use rand::{rngs::StdRng, Rng, SeedableRng};
use zksync_types::Transaction;
use zksync_vm_benchmark_harness::{
    cut_to_allowed_bytecode_size, get_deploy_tx_with_gas_limit, get_transfer_tx, BenchmarkingVm,
};

const GAS_LIMIT: u32 = 30_000_000;
const TXS_IN_BATCH: &[usize] = &[1, 10, 50, 100, 200, 500, 1_000, 2_000, 5_000];
const RNG_SEED: u64 = 123;
const TX_FAILURE_PROBABILITY: f64 = 0.2;

fn run_vm_expecting_failures<const FULL: bool>(
    group: &mut BenchmarkGroup<'_, WallTime>,
    name: &str,
    txs: &[Transaction],
    expected_failures: &[bool],
) {
    for txs_in_batch in TXS_IN_BATCH {
        group.throughput(Throughput::Elements(*txs_in_batch as u64));
        group.bench_with_input(
            BenchmarkId::new(name, txs_in_batch),
            txs_in_batch,
            |bencher, &txs_in_batch| {
                bencher.iter(|| {
                    let mut vm = BenchmarkingVm::new();
                    for (i, tx) in txs[..txs_in_batch].iter().enumerate() {
                        let result = if FULL {
                            vm.run_transaction_full(black_box(tx))
                        } else {
                            vm.run_transaction(black_box(tx))
                        };
                        let result = result.result;
                        let expecting_failure = expected_failures.get(i).copied().unwrap_or(false);
                        assert_eq!(
                            result.is_failed(),
                            expecting_failure,
                            "{result:?} on tx #{i}"
                        );
                    }
                })
            },
        );
    }
}

fn run_vm<const FULL: bool>(
    group: &mut BenchmarkGroup<'_, WallTime>,
    name: &str,
    txs: &[Transaction],
) {
    run_vm_expecting_failures::<FULL>(group, name, txs, &[]);
}

fn bench_fill_bootloader<const FULL: bool>(c: &mut Criterion) {
    let mut group = c.benchmark_group(if FULL {
        "fill_bootloader_full"
    } else {
        "fill_bootloader"
    });
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

    run_vm::<FULL>(&mut group, "deploy_simple_contract", &txs);
    drop(txs);

    let txs: Vec<_> = (0..max_txs).map(get_transfer_tx).collect();
    run_vm::<FULL>(&mut group, "transfer", &txs);

    // Halted transactions produced by the following tests *must* be rolled back,
    // otherwise the bootloader will process following transactions incorrectly.
    if !FULL {
        return;
    }

    let mut rng = StdRng::seed_from_u64(RNG_SEED);

    let mut txs_with_failures = Vec::with_capacity(txs.len());
    let mut expected_failures = Vec::with_capacity(txs.len());
    txs_with_failures.push(txs[0].clone());
    expected_failures.push(false);
    let mut successful_txs = &txs[1..];
    for _ in 1..txs.len() {
        let (tx, should_fail) = if rng.gen_bool(TX_FAILURE_PROBABILITY) {
            // Since we add the transaction with nonce 0 unconditionally as the first tx to execute,
            // all transactions generated here should fail
            (get_transfer_tx(0), true)
        } else {
            let (tx, remaining_txs) = successful_txs.split_first().unwrap();
            successful_txs = remaining_txs;
            (tx.clone(), false)
        };
        txs_with_failures.push(tx);
        expected_failures.push(should_fail);
    }
    run_vm_expecting_failures::<FULL>(
        &mut group,
        "transfer_with_invalid_nonce",
        &txs_with_failures,
        &expected_failures,
    );
}

criterion_group!(
    benches,
    bench_fill_bootloader::<false>,
    bench_fill_bootloader::<true>
);
criterion_main!(benches);

//! Benchmarks executing entire batches of transactions with varying size (from 1 to 5,000).
//!
//! - `fill_bootloader_full/*` benches emulate the entire transaction lifecycle including taking a snapshot
//!   before a transaction and rolling back to it on halt. They also include VM initialization and drop.
//!   In contrast, `fill_bootloader/*` benches only cover transaction execution.
//! - `deploy_simple_contract` benches deploy a simple contract in each transaction. All transactions succeed.
//! - `transfer` benches perform the base token transfer in each transaction. All transactions succeed.
//! - `transfer_with_invalid_nonce` benches are similar to `transfer`, but each transaction with a probability
//!   `TX_FAILURE_PROBABILITY` has a previously used nonce and thus halts during validation.
//! - `load_test(|_realistic|_heavy)` execute the load test contract (a mixture of storage reads, writes, emitting events,
//!   recursive calls, hashing and deploying new contracts). These 3 categories differ in how many operations of each kind
//!   are performed in each transaction. Beware that the first executed transaction is load test contract deployment,
//!   which skews results for small-size batches.

use std::{iter, time::Duration};

use criterion::{
    black_box, criterion_group, criterion_main, measurement::WallTime, BatchSize, BenchmarkGroup,
    BenchmarkId, Criterion, Throughput,
};
use rand::{rngs::StdRng, Rng, SeedableRng};
use zksync_types::Transaction;
use zksync_vm_benchmark_harness::{
    cut_to_allowed_bytecode_size, get_deploy_tx_with_gas_limit, get_heavy_load_test_tx,
    get_load_test_deploy_tx, get_load_test_tx, get_realistic_load_test_tx, get_transfer_tx,
    BenchmarkingVm, BenchmarkingVmFactory, Fast, Legacy, LoadTestParams,
};

/// Gas limit for deployment transactions.
const DEPLOY_GAS_LIMIT: u32 = 30_000_000;
/// Tested numbers of transactions in a batch.
const TXS_IN_BATCH: &[usize] = &[1, 10, 50, 100, 200, 500, 1_000, 2_000, 5_000];

/// RNG seed used e.g. to randomize failing transactions.
const RNG_SEED: u64 = 123;
/// Probability for a transaction to fail in the `transfer_with_invalid_nonce` benchmarks.
const TX_FAILURE_PROBABILITY: f64 = 0.2;

fn bench_vm<VM: BenchmarkingVmFactory, const FULL: bool>(
    vm: &mut BenchmarkingVm<VM>,
    txs: &[Transaction],
    expected_failures: &[bool],
) {
    for (i, tx) in txs.iter().enumerate() {
        let result = if FULL {
            vm.run_transaction_full(black_box(tx))
        } else {
            vm.run_transaction(black_box(tx))
        };
        let result = &result.result;
        let expecting_failure = expected_failures.get(i).copied().unwrap_or(false);
        assert_eq!(
            result.is_failed(),
            expecting_failure,
            "{result:?} on tx #{i}"
        );
        black_box(result);
    }
}

fn run_vm_expecting_failures<VM: BenchmarkingVmFactory, const FULL: bool>(
    group: &mut BenchmarkGroup<'_, WallTime>,
    name: &str,
    txs: &[Transaction],
    expected_failures: &[bool],
) {
    for txs_in_batch in TXS_IN_BATCH {
        if *txs_in_batch > txs.len() {
            break;
        }

        group.throughput(Throughput::Elements(*txs_in_batch as u64));
        group.bench_with_input(
            BenchmarkId::new(name, txs_in_batch),
            txs_in_batch,
            |bencher, &txs_in_batch| {
                if FULL {
                    // Include VM initialization / drop into the measured time
                    bencher.iter(|| {
                        let mut vm = BenchmarkingVm::<VM>::default();
                        bench_vm::<_, true>(&mut vm, &txs[..txs_in_batch], expected_failures);
                    });
                } else {
                    bencher.iter_batched(
                        BenchmarkingVm::<VM>::default,
                        |mut vm| {
                            bench_vm::<_, false>(&mut vm, &txs[..txs_in_batch], expected_failures);
                            vm
                        },
                        BatchSize::LargeInput, // VM can consume significant amount of RAM, especially the new one
                    );
                }
            },
        );
    }
}

fn run_vm<VM: BenchmarkingVmFactory, const FULL: bool>(
    group: &mut BenchmarkGroup<'_, WallTime>,
    name: &str,
    txs: &[Transaction],
) {
    run_vm_expecting_failures::<VM, FULL>(group, name, txs, &[]);
}

fn bench_fill_bootloader<VM: BenchmarkingVmFactory, const FULL: bool>(c: &mut Criterion) {
    let is_test_mode = !std::env::args().any(|arg| arg == "--bench");
    let txs_in_batch = if is_test_mode {
        &TXS_IN_BATCH[..3] // Reduce the number of transactions in a batch so that tests don't take long
    } else {
        TXS_IN_BATCH
    };

    let mut group = c.benchmark_group(if FULL {
        format!("fill_bootloader_full{}", VM::LABEL.as_suffix())
    } else {
        format!("fill_bootloader{}", VM::LABEL.as_suffix())
    });
    group
        .sample_size(10)
        .measurement_time(Duration::from_secs(10));

    // Deploying simple contract
    let test_contract =
        std::fs::read("deployment_benchmarks/deploy_simple_contract").expect("failed to read file");
    let code = cut_to_allowed_bytecode_size(&test_contract).unwrap();
    let max_txs = *txs_in_batch.last().unwrap() as u32;
    let txs: Vec<_> = (0..max_txs)
        .map(|nonce| get_deploy_tx_with_gas_limit(code, DEPLOY_GAS_LIMIT, nonce))
        .collect();
    run_vm::<VM, FULL>(&mut group, "deploy_simple_contract", &txs);
    drop(txs);

    // Load test with various parameters
    let txs =
        (1..=max_txs).map(|nonce| get_load_test_tx(nonce, 10_000_000, LoadTestParams::default()));
    let txs: Vec<_> = iter::once(get_load_test_deploy_tx()).chain(txs).collect();
    run_vm::<VM, FULL>(&mut group, "load_test", &txs);
    drop(txs);

    let txs = (1..=max_txs).map(get_realistic_load_test_tx);
    let txs: Vec<_> = iter::once(get_load_test_deploy_tx()).chain(txs).collect();
    run_vm::<VM, FULL>(&mut group, "load_test_realistic", &txs);
    drop(txs);

    let txs = (1..=max_txs).map(get_heavy_load_test_tx);
    let txs: Vec<_> = iter::once(get_load_test_deploy_tx()).chain(txs).collect();
    run_vm::<VM, FULL>(&mut group, "load_test_heavy", &txs);
    drop(txs);

    // Base token transfers
    let txs: Vec<_> = (0..max_txs).map(get_transfer_tx).collect();
    run_vm::<VM, FULL>(&mut group, "transfer", &txs);

    // Halted transactions produced by the following benchmarks *must* be rolled back,
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
            // all transactions generated here should halt during validation.
            (get_transfer_tx(0), true)
        } else {
            let (tx, remaining_txs) = successful_txs.split_first().unwrap();
            successful_txs = remaining_txs;
            (tx.clone(), false)
        };
        txs_with_failures.push(tx);
        expected_failures.push(should_fail);
    }
    run_vm_expecting_failures::<VM, FULL>(
        &mut group,
        "transfer_with_invalid_nonce",
        &txs_with_failures,
        &expected_failures,
    );
}

criterion_group!(
    benches,
    bench_fill_bootloader::<Fast, false>,
    bench_fill_bootloader::<Fast, true>,
    bench_fill_bootloader::<Legacy, false>
);
criterion_main!(benches);

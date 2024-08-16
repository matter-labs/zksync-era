use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use vm_benchmark::{
    criterion::{BenchmarkGroup, CriterionExt, MeteredTime},
    get_heavy_load_test_tx, get_load_test_deploy_tx, get_load_test_tx, get_realistic_load_test_tx,
    BenchmarkingVm, BenchmarkingVmFactory, Fast, Legacy, LoadTestParams, BYTECODES,
};
use zksync_types::Transaction;

const SAMPLE_SIZE: usize = 20;

fn benches_in_folder<VM: BenchmarkingVmFactory, const FULL: bool>(c: &mut Criterion<MeteredTime>) {
    let mut group = c.metered_group(VM::LABEL.as_str());
    group
        .sample_size(SAMPLE_SIZE)
        .measurement_time(Duration::from_secs(10));

    for bytecode in BYTECODES {
        let tx = bytecode.deploy_tx();
        let bench_name = bytecode.name;
        let full_suffix = if FULL { "/full" } else { "" };
        let bench_name = format!("{bench_name}{full_suffix}");

        group.bench_metered(bench_name, |bencher| {
            if FULL {
                // Include VM initialization / drop into the measured time
                bencher.iter(|| BenchmarkingVm::<VM>::default().run_transaction(black_box(&tx)));
            } else {
                bencher.iter_batched(
                    BenchmarkingVm::<VM>::default,
                    |mut vm| {
                        let result = vm.run_transaction(black_box(&tx));
                        (vm, result)
                    },
                    BatchSize::LargeInput, // VM can consume significant amount of RAM, especially the new one
                );
            }
        });
    }
}

fn bench_load_test<VM: BenchmarkingVmFactory>(c: &mut Criterion<MeteredTime>) {
    let mut group = c.metered_group(VM::LABEL.as_str());
    group
        .sample_size(SAMPLE_SIZE)
        .measurement_time(Duration::from_secs(10));

    // Nonce 0 is used for the deployment transaction
    let tx = get_load_test_tx(1, 10_000_000, LoadTestParams::default());
    bench_load_test_transaction::<VM>(&mut group, "load_test", &tx);

    let tx = get_realistic_load_test_tx(1);
    bench_load_test_transaction::<VM>(&mut group, "load_test_realistic", &tx);

    let tx = get_heavy_load_test_tx(1);
    bench_load_test_transaction::<VM>(&mut group, "load_test_heavy", &tx);
}

fn bench_load_test_transaction<VM: BenchmarkingVmFactory>(
    group: &mut BenchmarkGroup<'_>,
    name: &str,
    tx: &Transaction,
) {
    group.bench_metered(name, |bencher| {
        bencher.iter_batched(
            || {
                let mut vm = BenchmarkingVm::<VM>::default();
                vm.run_transaction(&get_load_test_deploy_tx());
                vm
            },
            |mut vm| {
                let result = vm.run_transaction(black_box(tx));
                assert!(!result.result.is_failed(), "{:?}", result.result);
                (vm, result)
            },
            BatchSize::LargeInput,
        );
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .configure_from_args()
        .with_measurement(MeteredTime::new("criterion"));
    targets = benches_in_folder::<Fast, false>,
        benches_in_folder::<Fast, true>,
        benches_in_folder::<Legacy, false>,
        benches_in_folder::<Legacy, true>,
        bench_load_test::<Fast>,
        bench_load_test::<Legacy>
);
criterion_main!(benches);

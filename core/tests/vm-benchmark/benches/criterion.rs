use std::time::Duration;

use criterion::{
    black_box, criterion_group, criterion_main, measurement::WallTime, BatchSize, BenchmarkGroup,
    Criterion,
};
use zksync_types::Transaction;
use zksync_vm_benchmark_harness::{
    cut_to_allowed_bytecode_size, get_deploy_tx, get_heavy_load_test_tx, get_load_test_deploy_tx,
    get_load_test_tx, get_realistic_load_test_tx, BenchmarkingVm, BenchmarkingVmFactory, Fast,
    Legacy, LoadTestParams,
};

const SAMPLE_SIZE: usize = 20;

fn benches_in_folder<VM: BenchmarkingVmFactory, const FULL: bool>(c: &mut Criterion) {
    let mut group = c.benchmark_group(VM::LABEL.as_str());
    group
        .sample_size(SAMPLE_SIZE)
        .measurement_time(Duration::from_secs(10));

    for path in std::fs::read_dir("deployment_benchmarks").unwrap() {
        let path = path.unwrap().path();

        let test_contract = std::fs::read(&path).expect("failed to read file");

        let code = cut_to_allowed_bytecode_size(&test_contract).unwrap();
        let tx = get_deploy_tx(code);
        let file_name = path.file_name().unwrap().to_str().unwrap();
        let full_suffix = if FULL { "/full" } else { "" };
        let bench_name = format!("{file_name}{full_suffix}");
        group.bench_function(bench_name, |bencher| {
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

fn bench_load_test<VM: BenchmarkingVmFactory>(c: &mut Criterion) {
    let mut group = c.benchmark_group(VM::LABEL.as_str());
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
    group: &mut BenchmarkGroup<'_, WallTime>,
    name: &str,
    tx: &Transaction,
) {
    group.bench_function(name, |bencher| {
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
    benches,
    benches_in_folder::<Fast, false>,
    benches_in_folder::<Fast, true>,
    benches_in_folder::<Legacy, false>,
    benches_in_folder::<Legacy, true>,
    bench_load_test::<Fast>,
    bench_load_test::<Legacy>
);
criterion_main!(benches);

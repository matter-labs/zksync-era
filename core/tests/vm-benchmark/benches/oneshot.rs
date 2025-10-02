use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use vm_benchmark::{
    criterion::{BenchmarkGroup, CriterionExt, MeteredTime},
    get_heavy_load_test_tx, get_load_test_deploy_tx, get_load_test_tx, get_realistic_load_test_tx,
    BenchmarkingVm, BenchmarkingVmFactory, Fast, FastNoSignatures, Legacy, LoadTestParams,
    BYTECODES,
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
                bencher.iter(|timer| {
                    let _guard = timer.start();
                    BenchmarkingVm::<VM>::default().run_transaction(black_box(&tx));
                });
            } else {
                bencher.iter(|timer| {
                    let mut vm = BenchmarkingVm::<VM>::default();
                    let guard = timer.start();
                    let _result = vm.run_transaction(black_box(&tx));
                    drop(guard); // do not include latency of dropping `_result`
                });
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
        bencher.iter(|timer| {
            let mut vm = BenchmarkingVm::<VM>::default();
            vm.run_transaction(&get_load_test_deploy_tx());

            let guard = timer.start();
            let result = vm.run_transaction(black_box(tx));
            drop(guard); // do not include the latency of `result` checks / drop
            assert!(!result.result.is_failed(), "{:?}", result.result);
        });
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .configure_from_args()
        .with_measurement(MeteredTime::new("criterion"));
    targets = benches_in_folder::<Fast, false>,
        benches_in_folder::<Fast, true>,
        benches_in_folder::<FastNoSignatures, false>,
        benches_in_folder::<Legacy, false>,
        benches_in_folder::<Legacy, true>,
        bench_load_test::<Fast>,
        bench_load_test::<FastNoSignatures>,
        bench_load_test::<Legacy>
);
criterion_main!(benches);

use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use zksync_vm_benchmark_harness::{
    cut_to_allowed_bytecode_size, get_deploy_tx, BenchmarkingVm, BenchmarkingVmFactory, Fast,
    Legacy,
};

const SAMPLE_SIZE: usize = 20;

fn benches_in_folder<VM: BenchmarkingVmFactory>(c: &mut Criterion) {
    let mut group = c.benchmark_group(VM::LABEL.as_str());
    group
        .sample_size(SAMPLE_SIZE)
        .measurement_time(Duration::from_secs(10));

    for path in std::fs::read_dir("deployment_benchmarks").unwrap() {
        let path = path.unwrap().path();

        let test_contract = std::fs::read(&path).expect("failed to read file");

        let code = cut_to_allowed_bytecode_size(&test_contract).unwrap();
        let tx = get_deploy_tx(code);
        let bench_name = path.file_name().unwrap().to_str().unwrap();
        group.bench_function(bench_name, |b| {
            b.iter(|| BenchmarkingVm::<VM>::default().run_transaction(black_box(&tx)))
        });
    }
}

criterion_group!(
    benches,
    benches_in_folder::<Fast>,
    benches_in_folder::<Legacy>
);
criterion_main!(benches);

use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use zksync_vm_benchmark_harness::{
    cut_to_allowed_bytecode_size, get_deploy_tx, BenchmarkingVm, BenchmarkingVmFactory, Fast,
    Legacy,
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
                        vm.run_transaction(black_box(&tx));
                        vm
                    },
                    BatchSize::LargeInput, // VM can consume significant amount of RAM, especially the new one
                );
            }
        });
    }
}

criterion_group!(
    benches,
    benches_in_folder::<Fast, false>,
    benches_in_folder::<Fast, true>,
    benches_in_folder::<Legacy, false>,
    benches_in_folder::<Legacy, true>
);
criterion_main!(benches);

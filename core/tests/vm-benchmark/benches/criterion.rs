use criterion::{black_box, criterion_group, criterion_main, Criterion};
use vm_benchmark_harness::{cut_to_allowed_bytecode_size, get_deploy_tx, BenchmarkingVm};

fn benches_in_folder(c: &mut Criterion) {
    for path in std::fs::read_dir("deployment_benchmarks").unwrap() {
        let path = path.unwrap().path();

        let test_contract = std::fs::read(&path).expect("failed to read file");

        let code = cut_to_allowed_bytecode_size(&test_contract).unwrap();
        let tx = get_deploy_tx(code);

        c.bench_function(path.file_name().unwrap().to_str().unwrap(), |b| {
            b.iter(|| BenchmarkingVm::new().run_transaction(black_box(&tx)))
        });
    }
}

criterion_group!(benches, benches_in_folder);
criterion_main!(benches);

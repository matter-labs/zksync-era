use criterion::black_box;
use std::time::{Duration, Instant};
use vm_benchmark_harness::{cut_to_allowed_bytecode_size, get_deploy_tx, BenchmarkingVm};

fn main() {
    let mut results = vec![];

    for path in std::fs::read_dir("deployment_benchmarks").unwrap() {
        let path = path.unwrap().path();

        let test_contract = std::fs::read(&path).expect("failed to read file");

        let code = cut_to_allowed_bytecode_size(&test_contract).unwrap();
        let tx = get_deploy_tx(code);

        let name = path.file_name().unwrap().to_str().unwrap();

        println!("benchmarking: {}", name);

        let mut timings = vec![];
        let benchmark_start = Instant::now();
        while benchmark_start.elapsed() < Duration::from_secs(5) {
            let start = Instant::now();
            BenchmarkingVm::new().run_transaction(black_box(&tx));
            timings.push(start.elapsed());
        }

        println!("{:?}", timings.iter().min().unwrap());
        results.push((name.to_owned(), timings));
    }

    if option_env!("PUSH_VM_BENCHMARKS_TO_PROMETHEUS").is_some() {
        vm_benchmark::with_prometheus::with_prometheus(|| {
            for (name, timings) in results {
                for (i, timing) in timings.into_iter().enumerate() {
                    metrics::gauge!("vm_benchmark.timing", timing, "benchmark" => name.clone(), "run_no" => i.to_string());
                }
            }
        });
    }
}

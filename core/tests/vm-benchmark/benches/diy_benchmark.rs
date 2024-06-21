use std::time::{Duration, Instant};

use criterion::black_box;
use vise::{Gauge, LabeledFamily, Metrics};
use zksync_vm_benchmark_harness::{cut_to_allowed_bytecode_size, get_deploy_tx, BenchmarkingVm};

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
        zksync_vm_benchmark::with_prometheus::with_prometheus(|| {
            for (name, timings) in results {
                for (i, timing) in timings.into_iter().enumerate() {
                    VM_BENCHMARK_METRICS.timing[&(name.clone(), i.to_string())].set(timing);
                }
            }
        });
    }
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "vm_benchmark")]
pub(crate) struct VmBenchmarkMetrics {
    #[metrics(labels = ["benchmark", "run_no"])]
    pub timing: LabeledFamily<(String, String), Gauge<Duration>, 2>,
}

#[vise::register]
pub(crate) static VM_BENCHMARK_METRICS: vise::Global<VmBenchmarkMetrics> = vise::Global::new();

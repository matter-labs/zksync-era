use std::time::Instant;

use criterion::black_box;
use vm_benchmark_harness::{
    cut_to_allowed_bytecode_size, get_deploy_tx_with_gas_limit, BenchmarkingVm,
};

fn main() {
    let test_contract =
        std::fs::read("deployment_benchmarks/event_spam").expect("failed to read file");

    let code = cut_to_allowed_bytecode_size(&test_contract).unwrap();
    let tx = get_deploy_tx_with_gas_limit(code, 1000);

    let start = Instant::now();

    let mut vm = BenchmarkingVm::new();
    for _ in 0..1000 {
        vm.run_transaction(black_box(&tx));
        dbg!("tx");
    }

    println!("{:?}", start.elapsed());
}

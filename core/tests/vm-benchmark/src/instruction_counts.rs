use vm_benchmark_harness::{cut_to_allowed_bytecode_size, get_deploy_tx, BenchmarkingVm};

fn main() {
    for path in std::fs::read_dir("deployment_benchmarks").unwrap() {
        let path = path.unwrap().path();

        let test_contract = std::fs::read(&path).expect("failed to read file");

        let code = cut_to_allowed_bytecode_size(&test_contract).unwrap();
        let tx = get_deploy_tx(code);

        let name = path.file_name().unwrap().to_str().unwrap();

        println!("{} {}", name, BenchmarkingVm::new().instruction_count(&tx));
    }
}

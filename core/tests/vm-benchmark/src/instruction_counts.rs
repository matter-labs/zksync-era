//! Runs all benchmarks and prints out the number of zkEVM opcodes each one executed.

use std::path::Path;

use zksync_vm_benchmark_harness::{cut_to_allowed_bytecode_size, get_deploy_tx, BenchmarkingVm};

fn main() {
    // using source file location because this is just a script, the binary isn't meant to be reused
    let benchmark_folder = Path::new(file!())
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("deployment_benchmarks");

    for path in std::fs::read_dir(benchmark_folder).unwrap() {
        let path = path.unwrap().path();

        let test_contract = std::fs::read(&path).expect("failed to read file");

        let code = cut_to_allowed_bytecode_size(&test_contract).unwrap();
        let tx = get_deploy_tx(code);

        let name = path.file_name().unwrap().to_str().unwrap();

        println!("{} {}", name, BenchmarkingVm::new().instruction_count(&tx));
    }
}

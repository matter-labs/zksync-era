//! Runs all benchmarks and prints out the number of zkEVM opcodes each one executed.

use vm_benchmark::{BenchmarkingVm, BYTECODES};

fn main() {
    for bytecode in BYTECODES {
        let tx = bytecode.deploy_tx();
        let name = bytecode.name;
        println!("{name} {}", BenchmarkingVm::new().instruction_count(&tx));
    }
}

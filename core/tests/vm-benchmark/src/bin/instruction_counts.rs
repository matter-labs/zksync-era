//! Runs all benchmarks and prints out the number of zkEVM opcodes each one executed.

use vm_benchmark::{BenchmarkingVmFactory, Fast, Legacy, BYTECODES};

fn main() {
    for bytecode in BYTECODES {
        let tx = bytecode.deploy_tx();
        let name = bytecode.name;
        println!("{name} {}", Fast::<()>::count_instructions(&tx));
        println!(
            "{} {}",
            name.to_string() + "_legacy",
            Legacy::count_instructions(&tx)
        );
    }
}

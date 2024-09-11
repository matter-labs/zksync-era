use vm_benchmark::{BenchmarkingVm, Bytecode};

fn main() {
    let bytecode_name = std::env::args()
        .nth(1)
        .expect("please provide bytecode name, e.g. 'access_memory'");
    let tx = Bytecode::get(&bytecode_name).deploy_tx();
    for _ in 0..100 {
        let mut vm = BenchmarkingVm::new();
        vm.run_transaction(&tx);
    }
}

use zksync_vm_benchmark_harness::*;

fn main() {
    let test_contract = std::fs::read(
        std::env::args()
            .nth(1)
            .expect("please provide an input file"),
    )
    .expect("failed to read file");

    let code = cut_to_allowed_bytecode_size(&test_contract).unwrap();
    let tx = get_deploy_tx(code);

    for _ in 0..100 {
        let mut vm = BenchmarkingVm::new();
        vm.run_transaction(&tx);
    }
}

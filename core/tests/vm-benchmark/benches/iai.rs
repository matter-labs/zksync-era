use vm_benchmark::{BenchmarkingVm, BenchmarkingVmFactory, Fast, Legacy, BYTECODES};
use yab::{Bencher, BenchmarkId};

fn benchmarks_for_vm<VM: BenchmarkingVmFactory>(bencher: &mut Bencher) {
    bencher.bench(
        BenchmarkId::new("init", VM::LABEL.as_str()),
        BenchmarkingVm::<VM>::default,
    );

    for bytecode in BYTECODES {
        let tx = bytecode.deploy_tx();
        bencher.bench_with_capture(
            BenchmarkId::new(bytecode.name, VM::LABEL.as_str()),
            |capture| {
                let mut vm = BenchmarkingVm::<VM>::default();
                capture.measure(|| vm.run_transaction(yab::black_box(&tx)));
            },
        );
    }
}

fn benchmarks(bencher: &mut Bencher) {
    // FIXME: integrate reporting here
    benchmarks_for_vm::<Fast>(bencher);
    benchmarks_for_vm::<Legacy>(bencher);
}

yab::main!(benchmarks);

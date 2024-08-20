use iai_callgrind::{black_box, library_benchmark, library_benchmark_group, LibraryBenchmarkConfig};
use zksync_types::Transaction;
use zksync_vm_benchmark_harness::{
    cut_to_allowed_bytecode_size, get_deploy_tx, BenchmarkingVm, BenchmarkingVmFactory, Fast,
    Legacy,
};

const FAST: Fast = Fast::new();
const LEGACY: Legacy = Legacy::new();

fn build_vm<VM: BenchmarkingVmFactory>(_factory: VM) -> BenchmarkingVm<VM> {
    BenchmarkingVm::<VM>::default()
}

fn prepare_tx(contract: &[u8]) -> Transaction {
    let code = cut_to_allowed_bytecode_size(contract).unwrap();
    get_deploy_tx(code)
}

fn run_bytecode<VM: BenchmarkingVmFactory>(mut vm: BenchmarkingVm::<VM>, tx: Transaction) {
    black_box(vm.run_transaction(&tx));
}

#[library_benchmark]
#[bench::fast(FAST)]
#[bench::legacy(LEGACY)]
fn bench_constructor<VM: BenchmarkingVmFactory>(_factory: VM) -> BenchmarkingVm<VM> {
    black_box(black_box(BenchmarkingVm::<VM>::default)())
}

macro_rules! load_file {
    ($file:ident) => {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/deployment_benchmarks/", stringify!($file)))
    }
}

macro_rules! make_functions_and_main {
    ($($file:ident => $legacy_name:ident,)+) => {
        $(
        #[library_benchmark]
        #[bench::$file(build_vm(FAST), prepare_tx(load_file!($file)))]
        #[bench::$legacy_name(build_vm(LEGACY), prepare_tx(load_file!($file)))]
        fn $file<VM: BenchmarkingVmFactory>(vm: BenchmarkingVm::<VM>, tx: Transaction) {
            run_bytecode(vm, tx);
        }
        )+
    };
}

make_functions_and_main!(
    access_memory => access_memory_legacy,
    call_far => call_far_legacy,
    decode_shl_sub => decode_shl_sub_legacy,
    deploy_simple_contract => deploy_simple_contract_legacy,
    finish_eventful_frames => finish_eventful_frames_legacy,
    write_and_decode => write_and_decode_legacy,
    event_spam => event_spam_legacy,
    slot_hash_collision => slot_hash_collision_legacy,
);

library_benchmark_group!(name = constructor; benchmarks = bench_constructor);
library_benchmark_group!(
    name = execution;
    benchmarks = access_memory, call_far, decode_shl_sub, deploy_simple_contract, finish_eventful_frames, write_and_decode, event_spam, slot_hash_collision
);

iai_callgrind::main!(
    config = LibraryBenchmarkConfig::default().env_clear(false);
    library_benchmark_groups = constructor, execution,
);

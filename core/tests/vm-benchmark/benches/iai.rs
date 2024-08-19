use iai::black_box;
use zksync_vm_benchmark_harness::{
    cut_to_allowed_bytecode_size, get_deploy_tx, BenchmarkingVm, BenchmarkingVmFactory, Fast,
    Legacy,
};

fn run_bytecode<VM: BenchmarkingVmFactory>(path: &str) {
    let test_contract = std::fs::read(path).expect("failed to read file");
    let code = cut_to_allowed_bytecode_size(&test_contract).unwrap();
    let tx = get_deploy_tx(code);

    black_box(BenchmarkingVm::<VM>::default().run_transaction(&tx));
}

macro_rules! make_functions_and_main {
    ($($file:ident => $legacy_name:ident,)+) => {
        $(
        fn $file() {
            run_bytecode::<Fast>(concat!("deployment_benchmarks/", stringify!($file)));
        }

        fn $legacy_name() {
            run_bytecode::<Legacy>(concat!("deployment_benchmarks/", stringify!($file)));
        }
        )+

        iai::main!($($file, $legacy_name,)+);
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

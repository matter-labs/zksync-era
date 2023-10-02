use iai::black_box;
use vm_benchmark_harness::{cut_to_allowed_bytecode_size, get_deploy_tx, BenchmarkingVm};

fn run_bytecode(path: &str) {
    let test_contract = std::fs::read(path).expect("failed to read file");
    let code = cut_to_allowed_bytecode_size(&test_contract).unwrap();
    let tx = get_deploy_tx(code);

    black_box(BenchmarkingVm::new().run_transaction(&tx));
}

macro_rules! make_functions_and_main {
    ($($file:ident,)+) => {
        $(
            fn $file() {
                run_bytecode(concat!("deployment_benchmarks/", stringify!($file)))
            }
        )+

        iai::main!($($file,)+);
    };
}

make_functions_and_main!(
    access_memory,
    call_far,
    decode_shl_sub,
    deploy_simple_contract,
    finish_eventful_frames,
    write_and_decode,
    event_spam,
    slot_hash_collision,
);

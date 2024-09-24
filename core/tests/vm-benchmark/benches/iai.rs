use iai::black_box;
use vm_benchmark::{BenchmarkingVm, BenchmarkingVmFactory, Bytecode, Fast, Legacy};

fn run_bytecode<VM: BenchmarkingVmFactory>(name: &str) {
    let tx = Bytecode::get(name).deploy_tx();
    black_box(BenchmarkingVm::<VM>::default().run_transaction(&tx));
}

macro_rules! make_functions_and_main {
    ($($file:ident => $legacy_name:ident,)+) => {
        $(
        fn $file() {
            run_bytecode::<Fast>(stringify!($file));
        }

        fn $legacy_name() {
            run_bytecode::<Legacy>(stringify!($file));
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

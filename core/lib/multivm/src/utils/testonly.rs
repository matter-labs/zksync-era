use zksync_types::zk_evm_types::FarCallOpcode;
use zksync_vm_interface::{Call, CallType};

pub(crate) fn check_call_tracer_test_result(call_tracer_result: &[Call]) {
    assert!(call_tracer_result.len() > 10);

    for call in call_tracer_result {
        check_call(call);
    }
}

fn check_call(call: &Call) {
    assert!(call.gas_used > call.calls.iter().map(|call| call.gas_used).sum::<u64>());

    for subcall in &call.calls {
        if subcall.r#type != CallType::Call(FarCallOpcode::Mimic) {
            assert_eq!(call.to, subcall.from);
        }
        check_call(subcall);
    }
}

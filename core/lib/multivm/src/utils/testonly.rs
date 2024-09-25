use std::fs;

use pretty_assertions::assert_eq;
use zksync_types::zk_evm_types::FarCallOpcode;
use zksync_vm_interface::{Call, CallType};

pub(crate) fn check_call_tracer_test_result(call_tracer_result: &[Call]) {
    assert!(call_tracer_result.len() > 10);

    for call in call_tracer_result {
        check_call(call);
    }

    if false {
        fs::write(
            "call_tracer_test_output",
            serde_json::to_string_pretty(call_tracer_result).unwrap(),
        )
        .unwrap();
    } else {
        let reference: Vec<Call> =
            serde_json::from_str(include_str!("call_tracer_test_output")).unwrap();
        assert_eq!(call_tracer_result, reference);
    }
}

fn check_call(call: &Call) {
    assert!(call.gas_used < call.gas);
    assert!(call.gas_used > call.calls.iter().map(|call| call.gas_used).sum::<u64>());

    for subcall in &call.calls {
        if subcall.r#type != CallType::Call(FarCallOpcode::Mimic) {
            assert_eq!(call.to, subcall.from);
        }
        check_call(subcall);
    }
}

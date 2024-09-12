use serde::{Deserialize, Serialize};
use zksync_basic_types::{web3::Bytes, U256};

use crate::{
    api::{DebugCall, DebugCallType, ResultDebugCall},
    Address,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugCallFlat {
    pub action: Action,
    pub result: CallResult,
    pub subtraces: usize,
    pub traceaddress: Vec<usize>,
    pub error: Option<String>,
    pub revert_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub r#type: DebugCallType,
    pub from: Address,
    pub to: Address,
    pub gas: U256,
    pub value: U256,
    pub input: Bytes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallResult {
    pub output: Bytes,
    pub gas_used: U256,
}

pub fn flatten_debug_calls(calls: Vec<ResultDebugCall>) -> Vec<DebugCallFlat> {
    let mut flattened_calls = Vec::new();
    for (index, result_debug_call) in calls.into_iter().enumerate() {
        let mut trace_address = vec![index]; // Initialize the trace addressees with the index of the top-level call
        flatten_call_recursive(
            &result_debug_call.result,
            &mut flattened_calls,
            &mut trace_address,
        );
    }
    flattened_calls
}

fn flatten_call_recursive(
    call: &DebugCall,
    flattened_calls: &mut Vec<DebugCallFlat>,
    trace_address: &mut Vec<usize>,
) {
    let flat_call = DebugCallFlat {
        action: Action {
            r#type: call.r#type.clone(),
            from: call.from,
            to: call.to,
            gas: call.gas,
            value: call.value,
            input: call.input.clone(),
        },
        result: CallResult {
            output: call.output.clone(),
            gas_used: call.gas_used,
        },
        subtraces: call.calls.len(),
        traceaddress: trace_address.clone(), // Clone the current trace address
        error: call.error.clone(),
        revert_reason: call.revert_reason.clone(),
    };
    flattened_calls.push(flat_call);

    // Process nested calls
    for (index, nested_call) in call.calls.iter().enumerate() {
        trace_address.push(index); // Update trace addressees for the nested call
        flatten_call_recursive(nested_call, flattened_calls, trace_address);
        trace_address.pop(); // Reset trace addressees after processing the nested call (prevent to keep filling the vector)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        api::{DebugCall, DebugCallType, ResultDebugCall},
        Address, BOOTLOADER_ADDRESS,
    };

    #[test]
    fn test_flatten_debug_call() {
        let result_debug_trace: Vec<ResultDebugCall> = [1, 1]
            .map(|_| ResultDebugCall {
                result: new_testing_debug_call(),
            })
            .into();

        let debug_call_flat = flatten_debug_calls(result_debug_trace);
        let expected_debug_call_flat = expected_flat_trace();
        assert_eq!(debug_call_flat, expected_debug_call_flat);
    }

    fn new_testing_debug_call() -> DebugCall {
        DebugCall {
            r#type: DebugCallType::Call,
            from: Address::zero(),
            to: BOOTLOADER_ADDRESS,
            gas: 1000.into(),
            gas_used: 1000.into(),
            value: 0.into(),
            output: vec![].into(),
            input: vec![].into(),
            error: None,
            revert_reason: None,
            calls: new_testing_trace(),
        }
    }

    fn new_testing_trace() -> Vec<DebugCall> {
        let first_call_trace = DebugCall {
            from: Address::zero(),
            to: Address::zero(),
            gas: 100.into(),
            gas_used: 42.into(),
            ..DebugCall::default()
        };
        let second_call_trace = DebugCall {
            from: Address::zero(),
            to: Address::zero(),
            value: 123.into(),
            gas: 58.into(),
            gas_used: 10.into(),
            input: Bytes(b"input".to_vec()),
            output: Bytes(b"output".to_vec()),
            ..DebugCall::default()
        };
        [first_call_trace, second_call_trace].into()
    }

    fn expected_flat_trace() -> Vec<DebugCallFlat> {
        [
            DebugCallFlat {
                action: Action {
                    r#type: DebugCallType::Call,
                    from: Address::zero(),
                    to: BOOTLOADER_ADDRESS,
                    gas: 1000.into(),
                    value: 0.into(),
                    input: vec![].into(),
                },
                result: CallResult {
                    output: vec![].into(),
                    gas_used: 1000.into(),
                },
                subtraces: 2,
                traceaddress: [0].into(),
                error: None,
                revert_reason: None,
            },
            DebugCallFlat {
                action: Action {
                    r#type: DebugCallType::Call,
                    from: Address::zero(),
                    to: Address::zero(),
                    gas: 100.into(),
                    value: 0.into(),
                    input: vec![].into(),
                },
                result: CallResult {
                    output: vec![].into(),
                    gas_used: 42.into(),
                },
                subtraces: 0,
                traceaddress: [0, 0].into(),
                error: None,
                revert_reason: None,
            },
            DebugCallFlat {
                action: Action {
                    r#type: DebugCallType::Call,
                    from: Address::zero(),
                    to: Address::zero(),
                    gas: 58.into(),
                    value: 123.into(),
                    input: b"input".to_vec().into(),
                },
                result: CallResult {
                    output: b"output".to_vec().into(),
                    gas_used: 10.into(),
                },
                subtraces: 0,
                traceaddress: [0, 1].into(),
                error: None,
                revert_reason: None,
            },
            DebugCallFlat {
                action: Action {
                    r#type: DebugCallType::Call,
                    from: Address::zero(),
                    to: BOOTLOADER_ADDRESS,
                    gas: 1000.into(),
                    value: 0.into(),
                    input: vec![].into(),
                },
                result: CallResult {
                    output: vec![].into(),
                    gas_used: 1000.into(),
                },
                subtraces: 2,
                traceaddress: [1].into(),
                error: None,
                revert_reason: None,
            },
            DebugCallFlat {
                action: Action {
                    r#type: DebugCallType::Call,
                    from: Address::zero(),
                    to: Address::zero(),
                    gas: 100.into(),
                    value: 0.into(),
                    input: vec![].into(),
                },
                result: CallResult {
                    output: vec![].into(),
                    gas_used: 42.into(),
                },
                subtraces: 0,
                traceaddress: [1, 0].into(),
                error: None,
                revert_reason: None,
            },
            DebugCallFlat {
                action: Action {
                    r#type: DebugCallType::Call,
                    from: Address::zero(),
                    to: Address::zero(),
                    gas: 58.into(),
                    value: 123.into(),
                    input: b"input".to_vec().into(),
                },
                result: CallResult {
                    output: b"output".to_vec().into(),
                    gas_used: 10.into(),
                },
                subtraces: 0,
                traceaddress: [1, 1].into(),
                error: None,
                revert_reason: None,
            },
        ]
        .into()
    }
}

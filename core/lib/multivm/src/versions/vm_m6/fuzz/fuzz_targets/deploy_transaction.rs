#![no_main]

use libfuzzer_sys::fuzz_target;
use vm_benchmark::{BenchmarkingVm, get_deploy_tx};
use zksync_types::tx::tx_execution_info::TxExecutionStatus::Success;

fuzz_target!(|input: &[u8]| {
    if let Some(contract_code) = cut_to_allowed_bytecode_size(input) {
        if let Ok(x) = BenchmarkingVm::new().run_transaction(&get_deploy_tx(contract_code)) {
            if x.status == Success {
                panic!("managed to produce valid code!");
            }
        }
    }
});

fn cut_to_allowed_bytecode_size(bytes: &[u8]) -> Option<&[u8]> {
    let mut words = bytes.len() / 32;
    if words == 0 {
        return None;
    }

    if words & 1 == 0 {
        words -= 1;
    }
    Some(&bytes[..32 * words])
}

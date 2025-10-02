use zksync_types::Transaction;

pub use crate::{
    transaction::{
        get_deploy_tx, get_deploy_tx_with_gas_limit, get_erc20_deploy_tx, get_erc20_transfer_tx,
        get_heavy_load_test_tx, get_load_test_deploy_tx, get_load_test_tx,
        get_realistic_load_test_tx, get_transfer_tx, LoadTestParams,
    },
    vm::{
        BenchmarkingVm, BenchmarkingVmFactory, CountInstructions, Fast, FastNoSignatures,
        FastWithStorageLimit, Legacy, VmLabel,
    },
};

pub mod criterion;
mod instruction_counter;
mod transaction;
mod vm;

#[derive(Debug, Clone, Copy)]
pub struct Bytecode {
    pub name: &'static str,
    raw_bytecode: &'static [u8],
}

impl Bytecode {
    pub fn get(name: &str) -> Self {
        BYTECODES
            .iter()
            .find(|bytecode| bytecode.name == name)
            .copied()
            .unwrap_or_else(|| panic!("bytecode `{name}` is not defined"))
    }

    /// Bytecodes must consist of an odd number of 32 byte words.
    /// This function "fixes" bytecodes of wrong length by cutting off their end.
    fn cut_to_allowed_bytecode_size(bytes: &[u8]) -> &[u8] {
        let mut words = bytes.len() / 32;
        assert!(words > 0, "bytecode is empty");

        if words & 1 == 0 {
            words -= 1;
        }
        &bytes[..32 * words]
    }

    pub fn bytecode(&self) -> &'static [u8] {
        Self::cut_to_allowed_bytecode_size(self.raw_bytecode)
    }

    pub fn deploy_tx(&self) -> Transaction {
        get_deploy_tx(self.bytecode())
    }
}

macro_rules! include_bytecode {
    ($name:ident) => {
        Bytecode {
            name: stringify!($name),
            raw_bytecode: include_bytes!(concat!("bytecodes/", stringify!($name))),
        }
    };
}

pub const BYTECODES: &[Bytecode] = &[
    include_bytecode!(access_memory),
    include_bytecode!(call_far),
    include_bytecode!(decode_shl_sub),
    include_bytecode!(deploy_simple_contract),
    include_bytecode!(event_spam),
    include_bytecode!(finish_eventful_frames),
    include_bytecode!(heap_read_write),
    include_bytecode!(slot_hash_collision),
    include_bytecode!(write_and_decode),
];

#[cfg(test)]
mod tests {
    use zksync_multivm::interface::{ExecutionResult, VmRevertReason};

    use super::*;

    #[test]
    fn deploy_transactions_are_valid() {
        for bytecode in BYTECODES {
            println!("Testing bytecode {}", bytecode.name);

            let mut vm = BenchmarkingVm::new();
            let res = vm.run_transaction(&bytecode.deploy_tx());
            match &res.result {
                ExecutionResult::Success { .. } => { /* OK */ }
                ExecutionResult::Revert {
                    output:
                        VmRevertReason::Unknown {
                            function_selector,
                            data,
                        },
                } if function_selector.is_empty() && data.is_empty() => {
                    // out of gas; this is expected for most fuzzed bytecodes
                }
                _ => panic!("Unexpected execution result: {:?}", res.result),
            }
        }
    }
}

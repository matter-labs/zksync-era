use crate::{
    versions::testonly::simple_execution::{
        test_estimate_fee, test_simple_execute, test_transfer_to_self_with_low_gas_limit,
    },
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn estimate_fee() {
    test_estimate_fee::<Vm<_, HistoryEnabled>>();
}

#[test]
fn simple_execute() {
    test_simple_execute::<Vm<_, HistoryEnabled>>();
}

#[test]
fn transfer_to_self_with_low_gas_limit() {
    test_transfer_to_self_with_low_gas_limit::<Vm<_, HistoryEnabled>>();
}

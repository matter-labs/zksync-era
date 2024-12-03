use crate::{
    versions::testonly::l1_tx_execution::{
        test_l1_tx_execution, test_l1_tx_execution_gas_estimation_with_low_gas,
        test_l1_tx_execution_high_gas_limit,
    },
    vm_fast::Vm,
};

#[test]
fn l1_tx_execution() {
    test_l1_tx_execution::<Vm<_>>();
}

#[test]
fn l1_tx_execution_high_gas_limit() {
    test_l1_tx_execution_high_gas_limit::<Vm<_>>();
}

#[test]
fn l1_tx_execution_gas_estimation_with_low_gas() {
    test_l1_tx_execution_gas_estimation_with_low_gas::<Vm<_>>();
}

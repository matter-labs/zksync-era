use crate::{
    versions::testonly::gas_limit::test_tx_gas_limit_offset,
    vm_1_5_0::{HistoryEnabled, Vm},
};

#[test]
fn tx_gas_limit_offset() {
    test_tx_gas_limit_offset::<Vm<_, HistoryEnabled>>();
}

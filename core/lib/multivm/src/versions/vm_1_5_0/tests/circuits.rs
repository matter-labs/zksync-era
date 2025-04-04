use crate::{
    versions::testonly::circuits::test_circuits,
    vm_1_5_0::{HistoryEnabled, Vm},
};

#[test]
fn circuits() {
    test_circuits::<Vm<_, HistoryEnabled>>();
}

use crate::{
    versions::testonly::circuits::test_circuits,
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn circuits() {
    test_circuits::<Vm<_, HistoryEnabled>>();
}

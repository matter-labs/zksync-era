use crate::{
    versions::testonly::require_eip712::test_require_eip712,
    vm_1_5_0::{HistoryEnabled, Vm},
};

#[test]
fn require_eip712() {
    test_require_eip712::<Vm<_, HistoryEnabled>>();
}

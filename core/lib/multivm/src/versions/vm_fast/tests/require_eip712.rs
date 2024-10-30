use crate::{versions::testonly::require_eip712::test_require_eip712, vm_fast::Vm};

#[test]
fn require_eip712() {
    test_require_eip712::<Vm<_>>();
}

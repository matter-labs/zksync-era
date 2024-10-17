use crate::{versions::testonly::circuits::test_circuits, vm_fast::Vm};

#[test]
fn circuits() {
    test_circuits::<Vm<_>>();
}

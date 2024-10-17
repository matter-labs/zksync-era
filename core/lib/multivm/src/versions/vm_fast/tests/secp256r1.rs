use crate::{versions::testonly::secp256r1::test_secp256r1, vm_fast::Vm};

#[test]
fn secp256r1() {
    test_secp256r1::<Vm<_>>();
}

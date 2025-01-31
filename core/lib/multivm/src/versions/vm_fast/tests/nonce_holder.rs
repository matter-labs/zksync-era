use crate::{versions::testonly::nonce_holder::test_nonce_holder, vm_fast::Vm};

#[test]
fn nonce_holder() {
    test_nonce_holder::<Vm<_>>();
}

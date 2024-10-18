use crate::{
    versions::testonly::nonce_holder::test_nonce_holder,
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn nonce_holder() {
    test_nonce_holder::<Vm<_, HistoryEnabled>>();
}

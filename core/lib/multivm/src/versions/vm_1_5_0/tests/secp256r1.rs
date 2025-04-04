use crate::{
    versions::testonly::secp256r1::test_secp256r1,
    vm_1_5_0::{HistoryEnabled, Vm},
};

#[test]
fn secp256r1() {
    test_secp256r1::<Vm<_, HistoryEnabled>>();
}

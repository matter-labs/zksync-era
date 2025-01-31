use crate::{
    versions::testonly::refunds::{
        test_negative_pubdata_for_transaction, test_predetermined_refunded_gas,
    },
    vm_fast::Vm,
};

#[test]
fn predetermined_refunded_gas() {
    test_predetermined_refunded_gas::<Vm<_>>();
}

#[test]
fn negative_pubdata_for_transaction() {
    test_negative_pubdata_for_transaction::<Vm<_>>();
}

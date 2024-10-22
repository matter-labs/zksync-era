use crate::{
    versions::testonly::precompiles::{test_ecrecover, test_keccak, test_sha256},
    vm_fast::Vm,
};

#[test]
fn keccak() {
    test_keccak::<Vm<_>>();
}

#[test]
fn sha256() {
    test_sha256::<Vm<_>>();
}

#[test]
fn ecrecover() {
    test_ecrecover::<Vm<_>>();
}

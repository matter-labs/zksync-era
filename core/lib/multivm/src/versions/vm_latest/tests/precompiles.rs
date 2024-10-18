use crate::{
    versions::testonly::precompiles::{test_ecrecover, test_keccak, test_sha256},
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn keccak() {
    test_keccak::<Vm<_, HistoryEnabled>>();
}

#[test]
fn sha256() {
    test_sha256::<Vm<_, HistoryEnabled>>();
}

#[test]
fn ecrecover() {
    test_ecrecover::<Vm<_, HistoryEnabled>>();
}

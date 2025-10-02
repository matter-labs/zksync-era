use crate::{
    versions::testonly::storage::{
        test_limiting_storage_reads, test_limiting_storage_writes, test_storage_behavior,
        test_transient_storage_behavior,
    },
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn storage_behavior() {
    test_storage_behavior::<Vm<_, HistoryEnabled>>();
}

#[test]
fn transient_storage_behavior() {
    test_transient_storage_behavior::<Vm<_, HistoryEnabled>>();
}

#[test]
fn limiting_storage_writes() {
    println!("Sanity check");
    test_limiting_storage_writes::<Vm<_, HistoryEnabled>>(false);
    println!("Storage limiter check");
    test_limiting_storage_writes::<Vm<_, HistoryEnabled>>(true);
}

#[test]
fn limiting_storage_reads() {
    println!("Sanity check");
    test_limiting_storage_reads::<Vm<_, HistoryEnabled>>(false);
    println!("Storage limiter check");
    test_limiting_storage_reads::<Vm<_, HistoryEnabled>>(true);
}

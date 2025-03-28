use crate::{
    versions::testonly::storage::{
        test_limiting_storage_reads, test_limiting_storage_writes, test_storage_behavior,
        test_transient_storage_behavior,
    },
    vm_fast::Vm,
};

#[test]
fn storage_behavior() {
    test_storage_behavior::<Vm<_>>();
}

#[test]
fn transient_storage_behavior() {
    test_transient_storage_behavior::<Vm<_>>();
}

#[test]
fn limiting_storage_writes() {
    println!("Sanity check");
    test_limiting_storage_writes::<Vm<_, _>>(false);
    println!("Storage limiter check");
    test_limiting_storage_writes::<Vm<_, _>>(true);
}

#[test]
fn limiting_storage_reads() {
    println!("Sanity check");
    test_limiting_storage_reads::<Vm<_, _>>(false);
    println!("Storage limiter check");
    test_limiting_storage_reads::<Vm<_, _>>(true);
}

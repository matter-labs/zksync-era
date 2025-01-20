use crate::versions::{testonly::call_tracer, vm_fast::Vm};

#[test]
fn basic_behavior() {
    call_tracer::test_basic_behavior::<Vm<_, _, _>>();
}

#[test]
fn transfer() {
    call_tracer::test_transfer::<Vm<_, _, _>>();
}

#[test]
fn reverted_tx() {
    call_tracer::test_reverted_tx::<Vm<_, _, _>>();
}

#[test]
fn reverted_deployment() {
    call_tracer::test_reverted_deployment_tx::<Vm<_, _, _>>();
}

#[test]
fn out_of_gas() {
    call_tracer::test_out_of_gas::<Vm<_, _, _>>();
}

#[test]
fn recursive_tx() {
    call_tracer::test_recursive_tx::<Vm<_, _, _>>();
}

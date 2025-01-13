use crate::versions::{testonly::call_tracer, vm_fast::Vm};

#[test]
fn basic_behavior() {
    call_tracer::test_basic_behavior::<Vm<_, _, _>>();
}

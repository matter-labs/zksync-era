use crate::{
    versions::testonly::tracing_execution_error::test_tracing_of_execution_errors, vm_fast::Vm,
};

#[test]
fn tracing_of_execution_errors() {
    test_tracing_of_execution_errors::<Vm<_>>();
}

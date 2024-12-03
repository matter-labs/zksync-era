use crate::{
    versions::testonly::tracing_execution_error::test_tracing_of_execution_errors,
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn tracing_of_execution_errors() {
    test_tracing_of_execution_errors::<Vm<_, HistoryEnabled>>();
}

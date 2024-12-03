use crate::{
    versions::testonly::is_write_initial::test_is_write_initial_behaviour,
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn is_write_initial_behaviour() {
    test_is_write_initial_behaviour::<Vm<_, HistoryEnabled>>();
}

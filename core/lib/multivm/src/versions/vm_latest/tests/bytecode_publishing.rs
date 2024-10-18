use crate::{
    versions::testonly::bytecode_publishing::test_bytecode_publishing,
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn bytecode_publishing() {
    test_bytecode_publishing::<Vm<_, HistoryEnabled>>();
}

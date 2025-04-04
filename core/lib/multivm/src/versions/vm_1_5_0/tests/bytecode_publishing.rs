use crate::{
    versions::testonly::bytecode_publishing::test_bytecode_publishing,
    vm_1_5_0::{HistoryEnabled, Vm},
};

#[test]
fn bytecode_publishing() {
    test_bytecode_publishing::<Vm<_, HistoryEnabled>>();
}

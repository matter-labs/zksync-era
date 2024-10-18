use crate::{versions::testonly::bytecode_publishing::test_bytecode_publishing, vm_fast::Vm};

#[test]
fn bytecode_publishing() {
    test_bytecode_publishing::<Vm<_>>();
}

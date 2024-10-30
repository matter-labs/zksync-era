use crate::{
    versions::testonly::account_validation_rules::test_account_validation_rules,
    vm_fast::{Vm, WithBuiltinTracersForValidation},
};

#[test]
fn test_account_validation_rules_fast() {
    test_account_validation_rules::<Vm<_, WithBuiltinTracersForValidation<()>>>();
}

use crate::{
    versions::testonly::account_validation_rules::test_account_validation_rules, vm_latest::Vm,
};

#[test]
fn test_account_validation_rules_legacy() {
    test_account_validation_rules::<Vm<_, _>>();
}

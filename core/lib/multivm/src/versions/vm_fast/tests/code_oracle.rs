use crate::{
    versions::testonly::code_oracle::{
        test_code_oracle, test_code_oracle_big_bytecode, test_refunds_in_code_oracle,
    },
    vm_fast::Vm,
};

#[test]
fn code_oracle() {
    test_code_oracle::<Vm<_>>();
}

#[test]
fn code_oracle_big_bytecode() {
    test_code_oracle_big_bytecode::<Vm<_>>();
}

#[test]
fn refunds_in_code_oracle() {
    test_refunds_in_code_oracle::<Vm<_>>();
}

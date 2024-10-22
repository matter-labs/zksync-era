use crate::{
    versions::testonly::code_oracle::{
        test_code_oracle, test_code_oracle_big_bytecode, test_refunds_in_code_oracle,
    },
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn code_oracle() {
    test_code_oracle::<Vm<_, HistoryEnabled>>();
}

#[test]
fn code_oracle_big_bytecode() {
    test_code_oracle_big_bytecode::<Vm<_, HistoryEnabled>>();
}

#[test]
fn refunds_in_code_oracle() {
    test_refunds_in_code_oracle::<Vm<_, HistoryEnabled>>();
}

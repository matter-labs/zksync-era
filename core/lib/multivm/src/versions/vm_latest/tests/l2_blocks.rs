use crate::{
    versions::testonly::l2_blocks::{
        test_l2_block_first_in_batch, test_l2_block_initialization_number_non_zero,
        test_l2_block_initialization_timestamp, test_l2_block_new_l2_block,
        test_l2_block_same_l2_block,
    },
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn l2_block_initialization_timestamp() {
    test_l2_block_initialization_timestamp::<Vm<_, HistoryEnabled>>();
}

#[test]
fn l2_block_initialization_number_non_zero() {
    test_l2_block_initialization_number_non_zero::<Vm<_, HistoryEnabled>>();
}

#[test]
fn l2_block_same_l2_block() {
    test_l2_block_same_l2_block::<Vm<_, HistoryEnabled>>();
}

#[test]
fn l2_block_new_l2_block() {
    test_l2_block_new_l2_block::<Vm<_, HistoryEnabled>>();
}

#[test]
fn l2_block_first_in_batch() {
    test_l2_block_first_in_batch::<Vm<_, HistoryEnabled>>();
}

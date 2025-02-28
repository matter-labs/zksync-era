use crate::{
    versions::testonly::v26_upgrade_utils::{
        test_post_bridging_test_storage_logs, test_post_registration_storage_logs,
        test_trivial_test_storage_logs,
    },
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn trivial_test_storage_logs() {
    test_trivial_test_storage_logs::<Vm<_, HistoryEnabled>>();
}

#[test]
fn post_bridging_test_storage_logs() {
    test_post_bridging_test_storage_logs::<Vm<_, HistoryEnabled>>();
}

#[test]
fn post_registration_storage_logs() {
    test_post_registration_storage_logs::<Vm<_, HistoryEnabled>>();
}

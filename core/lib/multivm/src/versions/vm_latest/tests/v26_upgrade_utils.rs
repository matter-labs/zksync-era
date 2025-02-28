use crate::{
    versions::testonly::v26_upgrade_utils::{
        test_post_bridging_test_storage_logs, test_post_registration_storage_logs,
        test_trivial_test_storage_logs, test_v26_unsafe_deposit_detection_trivial,
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

// #[tokio::test]
// async fn v26_unsafe_deposits_detection() {
//     test_v26_unsafe_deposit_detection_trivial::<Vm<_, HistoryEnabled>>().await;
// }

/*

142,74,35,214
8E4A23D6
*/

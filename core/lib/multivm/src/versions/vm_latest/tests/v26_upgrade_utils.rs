use crate::{versions::testonly::v26_upgrade_utils::test_v26_unsafe_deposit_detection_trivial, vm_latest::{HistoryEnabled, Vm}};

#[tokio::test]
async fn v26_unsafe_deposits_detection() {
    test_v26_unsafe_deposit_detection_trivial::<Vm<_, HistoryEnabled>>().await;
}
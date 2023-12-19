//! Miscellaneous utils used by multiple components.

use std::time::Duration;

use tokio::sync::watch;
use zksync_dal::ConnectionPool;
use zksync_types::L1BatchNumber;

/// Repeatedly polls DB until there is an L1 batch with metadata. We may not have such a batch
/// if the DB is recovered from an (application-level) snapshot.
///
/// Returns the number of rhe *earliest* L1 batch with metadata, or `None` if the stop signal is received.
pub(crate) async fn wait_for_l1_batch_with_metadata(
    pool: &ConnectionPool,
    poll_interval: Duration,
    stop_receiver: &watch::Receiver<bool>,
) -> anyhow::Result<Option<L1BatchNumber>> {
    loop {
        if *stop_receiver.borrow() {
            return Ok(None);
        }

        let mut storage = pool.access_storage().await?;
        let sealed_l1_batch_number = storage
            .blocks_dal()
            .get_earliest_l1_batch_number_with_metadata()
            .await?;
        drop(storage);

        if let Some(number) = sealed_l1_batch_number {
            return Ok(Some(number));
        }
        tokio::time::sleep(poll_interval).await;
    }
}

//! Various helpers for using DAL methods.

use std::time::Duration;

use tokio::sync::watch;
use zksync_types::{L1BatchNumber, OrStopped};

use crate::{ConnectionPool, Core, CoreDal};

/// Repeatedly polls the DB until there is an L1 batch. We may not have such a batch initially
/// if the DB is recovered from an application-level snapshot.
///
/// Returns the number of the *earliest* L1 batch.
pub async fn wait_for_l1_batch(
    pool: &ConnectionPool<Core>,
    poll_interval: Duration,
    stop_receiver: &mut watch::Receiver<bool>,
) -> Result<L1BatchNumber, OrStopped> {
    tracing::debug!("Waiting for at least one L1 batch in db in DB");
    loop {
        if *stop_receiver.borrow() {
            return Err(OrStopped::Stopped);
        }

        let mut storage = pool.connection().await?;
        let sealed_l1_batch_number = storage.blocks_dal().get_earliest_l1_batch_number().await?;
        drop(storage);

        if let Some(number) = sealed_l1_batch_number {
            return Ok(number);
        }

        // We don't check the result: if a stop request is received, we'll return at the start
        // of the next iteration.
        tokio::time::timeout(poll_interval, stop_receiver.changed())
            .await
            .ok();
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::ProtocolVersion;

    use super::*;
    use crate::{tests::create_l1_batch_header, ConnectionPool, Core, CoreDal};

    #[tokio::test]
    async fn waiting_for_l1_batch_success() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let (_stop_sender, mut stop_receiver) = watch::channel(false);

        let pool_copy = pool.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(25)).await;
            let mut conn = pool_copy.connection().await.unwrap();
            conn.protocol_versions_dal()
                .save_protocol_version_with_tx(&ProtocolVersion::default())
                .await
                .unwrap();
            let header = create_l1_batch_header(0);
            conn.blocks_dal()
                .insert_mock_l1_batch(&header)
                .await
                .unwrap();
        });

        let l1_batch = wait_for_l1_batch(&pool, Duration::from_millis(10), &mut stop_receiver)
            .await
            .unwrap();
        assert_eq!(l1_batch, L1BatchNumber(0));
    }

    #[tokio::test]
    async fn waiting_for_l1_batch_cancellation() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let (stop_sender, mut stop_receiver) = watch::channel(false);

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(25)).await;
            stop_sender.send_replace(true);
        });

        let err = wait_for_l1_batch(&pool, Duration::from_secs(30), &mut stop_receiver)
            .await
            .unwrap_err();
        assert!(matches!(err, OrStopped::Stopped));
    }
}

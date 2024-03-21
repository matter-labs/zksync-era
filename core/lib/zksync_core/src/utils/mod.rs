//! Miscellaneous utils used by multiple components.

use std::{
    future::Future,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_types::{L1BatchNumber, ProtocolVersionId};

#[cfg(test)]
pub(crate) mod testonly;

/// Fallible and async predicate for binary search.
#[async_trait]
pub(crate) trait BinarySearchPredicate: Send {
    type Error;

    async fn eval(&mut self, argument: u32) -> Result<bool, Self::Error>;
}

#[async_trait]
impl<F, Fut, E> BinarySearchPredicate for F
where
    F: Send + FnMut(u32) -> Fut,
    Fut: Send + Future<Output = Result<bool, E>>,
{
    type Error = E;

    async fn eval(&mut self, argument: u32) -> Result<bool, Self::Error> {
        self(argument).await
    }
}

/// Finds the greatest `u32` value for which `f` returns `true`.
pub(crate) async fn binary_search_with<P: BinarySearchPredicate>(
    mut left: u32,
    mut right: u32,
    mut predicate: P,
) -> Result<u32, P::Error> {
    while left + 1 < right {
        let middle = (left + right) / 2;
        if predicate.eval(middle).await? {
            left = middle;
        } else {
            right = middle;
        }
    }
    Ok(left)
}

/// Repeatedly polls the DB until there is an L1 batch. We may not have such a batch initially
/// if the DB is recovered from an application-level snapshot.
///
/// Returns the number of the *earliest* L1 batch, or `None` if the stop signal is received.
pub(crate) async fn wait_for_l1_batch(
    pool: &ConnectionPool<Core>,
    poll_interval: Duration,
    stop_receiver: &mut watch::Receiver<bool>,
) -> anyhow::Result<Option<L1BatchNumber>> {
    tracing::debug!("Waiting for at least one L1 batch in db in DB");
    loop {
        if *stop_receiver.borrow() {
            return Ok(None);
        }

        let mut storage = pool.connection().await?;
        let sealed_l1_batch_number = storage.blocks_dal().get_earliest_l1_batch_number().await?;
        drop(storage);

        if let Some(number) = sealed_l1_batch_number {
            return Ok(Some(number));
        }

        // We don't check the result: if a stop signal is received, we'll return at the start
        // of the next iteration.
        tokio::time::timeout(poll_interval, stop_receiver.changed())
            .await
            .ok();
    }
}

/// Repeatedly polls the DB until there is an L1 batch with metadata. We may not have such a batch initially
/// if the DB is recovered from an application-level snapshot.
///
/// Returns the number of the *earliest* L1 batch with metadata, or `None` if the stop signal is received.
pub(crate) async fn wait_for_l1_batch_with_metadata(
    pool: &ConnectionPool<Core>,
    poll_interval: Duration,
    stop_receiver: &mut watch::Receiver<bool>,
) -> anyhow::Result<Option<L1BatchNumber>> {
    loop {
        if *stop_receiver.borrow() {
            return Ok(None);
        }

        let mut storage = pool.connection().await?;
        let sealed_l1_batch_number = storage
            .blocks_dal()
            .get_earliest_l1_batch_number_with_metadata()
            .await?;
        drop(storage);

        if let Some(number) = sealed_l1_batch_number {
            return Ok(Some(number));
        }
        tracing::debug!(
            "No L1 batches with metadata are present in DB; trying again in {poll_interval:?}"
        );
        tokio::time::timeout(poll_interval, stop_receiver.changed())
            .await
            .ok();
    }
}

/// Returns the projected number of the first locally available L1 batch. The L1 batch is **not**
/// guaranteed to be present in the storage!
pub(crate) async fn projected_first_l1_batch(
    storage: &mut Connection<'_, Core>,
) -> anyhow::Result<L1BatchNumber> {
    let snapshot_recovery = storage
        .snapshot_recovery_dal()
        .get_applied_snapshot_status()
        .await
        .context("failed getting snapshot recovery status")?;
    Ok(snapshot_recovery.map_or(L1BatchNumber(0), |recovery| recovery.l1_batch_number + 1))
}

/// Obtains a protocol version projected to be applied for the next miniblock. This is either the version used by the last
/// sealed miniblock, or (if there are no miniblocks), one referenced in the snapshot recovery record.
pub(crate) async fn pending_protocol_version(
    storage: &mut Connection<'_, Core>,
) -> anyhow::Result<ProtocolVersionId> {
    static WARNED_ABOUT_NO_VERSION: AtomicBool = AtomicBool::new(false);

    let last_miniblock = storage
        .blocks_dal()
        .get_last_sealed_miniblock_header()
        .await
        .context("failed getting last sealed miniblock")?;
    if let Some(last_miniblock) = last_miniblock {
        return Ok(last_miniblock.protocol_version.unwrap_or_else(|| {
            // Protocol version should be set for the most recent miniblock even in cases it's not filled
            // for old miniblocks, hence the warning. We don't want to rely on this assumption, so we treat
            // the lack of it as in other similar places, replacing with the default value.
            if !WARNED_ABOUT_NO_VERSION.fetch_or(true, Ordering::Relaxed) {
                tracing::warn!("Protocol version not set for recent miniblock: {last_miniblock:?}");
            }
            ProtocolVersionId::last_potentially_undefined()
        }));
    }
    // No miniblocks in the storage; use snapshot recovery information.
    let snapshot_recovery = storage
        .snapshot_recovery_dal()
        .get_applied_snapshot_status()
        .await
        .context("failed getting snapshot recovery status")?
        .context("storage contains neither miniblocks, nor snapshot recovery info")?;
    Ok(snapshot_recovery.protocol_version)
}

#[cfg(test)]
mod tests {
    use zksync_types::L2ChainId;

    use super::*;
    use crate::genesis::{ensure_genesis_state, GenesisParams};

    #[tokio::test]
    async fn test_binary_search() {
        for divergence_point in [1, 50, 51, 100] {
            let mut f = |x| async move { Ok::<_, ()>(x < divergence_point) };
            let result = binary_search_with(0, 100, &mut f).await;
            assert_eq!(result, Ok(divergence_point - 1));
        }
    }

    #[tokio::test]
    async fn waiting_for_l1_batch_success() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let (_stop_sender, mut stop_receiver) = watch::channel(false);

        let pool_copy = pool.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(25)).await;
            let mut storage = pool_copy.connection().await.unwrap();
            ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
                .await
                .unwrap();
        });

        let l1_batch = wait_for_l1_batch(&pool, Duration::from_millis(10), &mut stop_receiver)
            .await
            .unwrap();
        assert_eq!(l1_batch, Some(L1BatchNumber(0)));
    }

    #[tokio::test]
    async fn waiting_for_l1_batch_cancellation() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let (stop_sender, mut stop_receiver) = watch::channel(false);

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(25)).await;
            stop_sender.send_replace(true);
        });

        let l1_batch = wait_for_l1_batch(&pool, Duration::from_secs(30), &mut stop_receiver)
            .await
            .unwrap();
        assert_eq!(l1_batch, None);
    }
}

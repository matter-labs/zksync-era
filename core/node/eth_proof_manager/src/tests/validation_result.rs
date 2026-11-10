use std::{num::NonZeroU32, time::Duration};

use zksync_config::configs::proof_data_handler::ProvingMode;
use zksync_dal::{eth_proof_manager_dal::ProvingNetwork, CoreDal};
use zksync_types::{L1BatchNumber, H256};

use crate::tests::TestContext;

const RETRY_BACKOFF: Duration = Duration::from_secs(1);

async fn acknowledged_batch(ctx: &TestContext) -> L1BatchNumber {
    let mut connection = ctx.connection_pool.connection().await.unwrap();

    let batch = connection
        .eth_proof_manager_dal()
        .lock_batch_for_proving_network_and_prepare(NonZeroU32::MIN)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(batch, L1BatchNumber(1));

    connection
        .eth_proof_manager_dal()
        .mark_batch_as_submitting(batch, "url")
        .await
        .unwrap();
    connection
        .eth_proof_manager_dal()
        .mark_batch_as_sent(batch, H256::zero())
        .await
        .unwrap();
    connection
        .eth_proof_manager_dal()
        .acknowledge_batch(batch, ProvingNetwork::Fermah)
        .await
        .unwrap();

    batch
}

async fn fallbacked_batch_with_proof_on_chain(ctx: &TestContext, is_proof_valid: bool) {
    let batch = acknowledged_batch(ctx).await;
    let mut connection = ctx.connection_pool.connection().await.unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;
    connection
        .eth_proof_manager_dal()
        .fallback_batches(
            ctx.config.acknowledgment_timeout,
            ctx.config.proof_generation_timeout,
            ctx.config.picking_timeout,
        )
        .await
        .unwrap();

    connection
        .eth_proof_manager_dal()
        .mark_batch_as_proven(batch, is_proof_valid, 10)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_invalid_proof_is_reported() {
    let ctx = TestContext::new().await.init().await;
    fallbacked_batch_with_proof_on_chain(&ctx, false).await;

    let next = ctx
        .connection_pool
        .connection()
        .await
        .unwrap()
        .eth_proof_manager_dal()
        .get_batch_to_send_validation_result(10, RETRY_BACKOFF)
        .await
        .unwrap();

    assert_eq!(next, Some((L1BatchNumber(1), false)));
}

#[tokio::test]
async fn test_nothing_to_report_is_not_an_error() {
    let ctx = TestContext::new().await.init().await;

    let next = ctx
        .connection_pool
        .connection()
        .await
        .unwrap()
        .eth_proof_manager_dal()
        .get_batch_to_send_validation_result(10, RETRY_BACKOFF)
        .await
        .unwrap();

    assert_eq!(next, None);
}

/// A verdict the contract keeps rejecting must leave the pool for good, including past
/// the backoff interval; otherwise it reverts once per interval forever.
#[tokio::test]
async fn test_batch_is_abandoned_after_attempts_are_exhausted() {
    let ctx = TestContext::new().await.init().await;
    fallbacked_batch_with_proof_on_chain(&ctx, false).await;

    let mut connection = ctx.connection_pool.connection().await.unwrap();

    let next = connection
        .eth_proof_manager_dal()
        .get_batch_to_send_validation_result(2, RETRY_BACKOFF)
        .await
        .unwrap();
    assert_eq!(next, Some((L1BatchNumber(1), false)));

    // Retries of the same batch wait out the backoff.
    let next = connection
        .eth_proof_manager_dal()
        .get_batch_to_send_validation_result(2, RETRY_BACKOFF)
        .await
        .unwrap();
    assert_eq!(next, None);

    tokio::time::sleep(RETRY_BACKOFF + Duration::from_secs(1)).await;
    let next = connection
        .eth_proof_manager_dal()
        .get_batch_to_send_validation_result(2, RETRY_BACKOFF)
        .await
        .unwrap();
    assert_eq!(next, Some((L1BatchNumber(1), false)));

    tokio::time::sleep(RETRY_BACKOFF + Duration::from_secs(1)).await;
    let next = connection
        .eth_proof_manager_dal()
        .get_batch_to_send_validation_result(2, RETRY_BACKOFF)
        .await
        .unwrap();
    assert_eq!(next, None);

    let stuck = connection
        .eth_proof_manager_dal()
        .get_reached_max_attempts_amount(2)
        .await
        .unwrap();
    assert_eq!(stuck, 1);
}

#[tokio::test]
async fn test_validated_batch_is_not_reported_again() {
    let ctx = TestContext::new().await.init().await;
    fallbacked_batch_with_proof_on_chain(&ctx, false).await;

    let mut connection = ctx.connection_pool.connection().await.unwrap();
    connection
        .eth_proof_manager_dal()
        .mark_batch_as_validated(L1BatchNumber(1), H256::repeat_byte(1))
        .await
        .unwrap();

    let next = connection
        .eth_proof_manager_dal()
        .get_batch_to_send_validation_result(10, RETRY_BACKOFF)
        .await
        .unwrap();

    assert_eq!(next, None);
}

/// A request already settled on chain must stop being selected, even though we never
/// landed a transaction of our own to record against it. Otherwise it reverts with
/// `ProofRequestIsNotProven(Validated)` once per backoff interval forever.
#[tokio::test]
async fn test_already_recorded_validation_is_not_retried() {
    let ctx = TestContext::new().await.init().await;
    fallbacked_batch_with_proof_on_chain(&ctx, false).await;

    let mut connection = ctx.connection_pool.connection().await.unwrap();

    let next = connection
        .eth_proof_manager_dal()
        .get_batch_to_send_validation_result(10, RETRY_BACKOFF)
        .await
        .unwrap();
    assert_eq!(next, Some((L1BatchNumber(1), false)));

    connection
        .eth_proof_manager_dal()
        .mark_batch_validation_already_recorded(L1BatchNumber(1))
        .await
        .unwrap();

    tokio::time::sleep(RETRY_BACKOFF + Duration::from_secs(1)).await;
    let next = connection
        .eth_proof_manager_dal()
        .get_batch_to_send_validation_result(10, RETRY_BACKOFF)
        .await
        .unwrap();
    assert_eq!(next, None);

    // No reward is locked in that state, so it must not be counted as one.
    let stuck = connection
        .eth_proof_manager_dal()
        .get_reached_max_attempts_amount(1)
        .await
        .unwrap();
    assert_eq!(stuck, 0);
}

#[tokio::test]
async fn test_acknowledgment_replay_does_not_regress_proven_batch() {
    let ctx = TestContext::new().await.init().await;
    let batch = acknowledged_batch(&ctx).await;

    let mut connection = ctx.connection_pool.connection().await.unwrap();
    connection
        .eth_proof_manager_dal()
        .mark_batch_as_proven(batch, false, 10)
        .await
        .unwrap();

    connection
        .eth_proof_manager_dal()
        .acknowledge_batch(batch, ProvingNetwork::Fermah)
        .await
        .unwrap();

    connection
        .eth_proof_manager_dal()
        .fallback_batches(
            ctx.config.acknowledgment_timeout,
            ctx.config.proof_generation_timeout,
            ctx.config.picking_timeout,
        )
        .await
        .unwrap();

    let next = connection
        .eth_proof_manager_dal()
        .get_batch_to_send_validation_result(10, RETRY_BACKOFF)
        .await
        .unwrap();

    assert_eq!(next, Some((batch, false)));
}

#[tokio::test]
async fn test_acknowledgment_replay_does_not_undo_fallback() {
    let ctx = TestContext::new().await.init().await;
    fallbacked_batch_with_proof_on_chain(&ctx, false).await;

    let mut connection = ctx.connection_pool.connection().await.unwrap();

    connection
        .eth_proof_manager_dal()
        .acknowledge_batch(L1BatchNumber(1), ProvingNetwork::Fermah)
        .await
        .unwrap();

    let next = connection
        .eth_proof_manager_dal()
        .get_batch_to_send_validation_result(10, RETRY_BACKOFF)
        .await
        .unwrap();

    assert_eq!(next, Some((L1BatchNumber(1), false)));
}

#[tokio::test]
async fn test_late_send_does_not_undo_fallback() {
    let ctx = TestContext::new().await.init().await;
    fallbacked_batch_with_proof_on_chain(&ctx, false).await;

    let mut connection = ctx.connection_pool.connection().await.unwrap();
    connection
        .eth_proof_manager_dal()
        .mark_batch_as_sent(L1BatchNumber(1), H256::repeat_byte(2))
        .await
        .unwrap();

    let next = connection
        .eth_proof_manager_dal()
        .get_batch_to_send_validation_result(10, RETRY_BACKOFF)
        .await
        .unwrap();

    assert_eq!(next, Some((L1BatchNumber(1), false)));
}

#[tokio::test]
async fn test_replay_does_not_overwrite_recorded_verdict() {
    let ctx = TestContext::new().await.init().await;
    fallbacked_batch_with_proof_on_chain(&ctx, true).await;

    let mut connection = ctx.connection_pool.connection().await.unwrap();

    connection
        .eth_proof_manager_dal()
        .mark_batch_as_proven(L1BatchNumber(1), false, 0)
        .await
        .unwrap();

    let next = connection
        .eth_proof_manager_dal()
        .get_batch_to_send_validation_result(10, RETRY_BACKOFF)
        .await
        .unwrap();
    assert_eq!(next, None);

    connection
        .eth_proof_manager_dal()
        .fallback_batches(
            ctx.config.acknowledgment_timeout,
            ctx.config.proof_generation_timeout,
            ctx.config.picking_timeout,
        )
        .await
        .unwrap();

    let next = connection
        .eth_proof_manager_dal()
        .get_batch_to_send_validation_result(10, RETRY_BACKOFF)
        .await
        .unwrap();
    assert_eq!(next, None);
}

#[tokio::test]
async fn test_rejection_replay_does_not_requeue_picked_batch() {
    let ctx = TestContext::new().await.init().await;
    let batch = acknowledged_batch(&ctx).await;

    let mut connection = ctx.connection_pool.connection().await.unwrap();

    connection
        .eth_proof_manager_dal()
        .fallback_batch(batch)
        .await
        .unwrap();

    // Exclude stale-job recovery from this replay check.
    let pick_timeout = Duration::from_secs(3600);
    let processor = ctx.processor(ProvingMode::ProverCluster).await;
    let picked = processor
        .lock_batch_for_proving(pick_timeout)
        .await
        .unwrap();
    assert_eq!(picked, Some(batch));

    connection
        .eth_proof_manager_dal()
        .fallback_batch(batch)
        .await
        .unwrap();

    let picked_again = processor
        .lock_batch_for_proving(pick_timeout)
        .await
        .unwrap();
    assert_eq!(picked_again, None);
}

#[tokio::test]
async fn test_event_replay_does_not_postpone_validation_retry() {
    let ctx = TestContext::new().await.init().await;
    fallbacked_batch_with_proof_on_chain(&ctx, false).await;

    let mut connection = ctx.connection_pool.connection().await.unwrap();

    let next = connection
        .eth_proof_manager_dal()
        .get_batch_to_send_validation_result(10, RETRY_BACKOFF)
        .await
        .unwrap();
    assert_eq!(next, Some((L1BatchNumber(1), false)));

    tokio::time::sleep(RETRY_BACKOFF + Duration::from_secs(1)).await;

    connection
        .eth_proof_manager_dal()
        .mark_batch_as_proven(L1BatchNumber(1), false, 10)
        .await
        .unwrap();

    let next = connection
        .eth_proof_manager_dal()
        .get_batch_to_send_validation_result(10, RETRY_BACKOFF)
        .await
        .unwrap();
    assert_eq!(next, Some((L1BatchNumber(1), false)));
}

#[tokio::test]
async fn test_invalid_verdict_waits_for_fallback() {
    let ctx = TestContext::new().await.init().await;
    let batch = acknowledged_batch(&ctx).await;
    let mut connection = ctx.connection_pool.connection().await.unwrap();

    connection
        .eth_proof_manager_dal()
        .mark_batch_as_proven(batch, false, 10)
        .await
        .unwrap();
    let next = connection
        .eth_proof_manager_dal()
        .get_batch_to_send_validation_result(10, RETRY_BACKOFF)
        .await
        .unwrap();
    assert_eq!(next, None);

    let timeout = Duration::from_secs(3600);
    let fallbacked = connection
        .eth_proof_manager_dal()
        .fallback_batches(timeout, timeout, timeout)
        .await
        .unwrap();
    assert_eq!(fallbacked, 1);

    let next = connection
        .eth_proof_manager_dal()
        .get_batch_to_send_validation_result(10, RETRY_BACKOFF)
        .await
        .unwrap();
    assert_eq!(next, Some((batch, false)));
    connection
        .eth_proof_manager_dal()
        .mark_batch_as_validated(batch, H256::repeat_byte(1))
        .await
        .unwrap();

    let processor = ctx.processor(ProvingMode::ProvingNetwork).await;
    let picked = processor.lock_batch_for_proving(timeout).await.unwrap();
    assert_eq!(picked, Some(batch));
}

#[tokio::test]
async fn test_early_acknowledgment_preserves_proving_timeout() {
    let ctx = TestContext::new().await.init().await;
    let mut connection = ctx.connection_pool.connection().await.unwrap();
    let batch = connection
        .eth_proof_manager_dal()
        .lock_batch_for_proving_network_and_prepare(NonZeroU32::MIN)
        .await
        .unwrap()
        .unwrap();
    connection
        .eth_proof_manager_dal()
        .mark_batch_as_submitting(batch, "url")
        .await
        .unwrap();
    connection
        .eth_proof_manager_dal()
        .acknowledge_batch(batch, ProvingNetwork::Fermah)
        .await
        .unwrap();
    connection
        .eth_proof_manager_dal()
        .mark_batch_as_sent(batch, H256::repeat_byte(2))
        .await
        .unwrap();

    let timeout = Duration::from_secs(3600);
    let fallbacked = connection
        .eth_proof_manager_dal()
        .fallback_batches(Duration::ZERO, timeout, timeout)
        .await
        .unwrap();
    assert_eq!(fallbacked, 0);

    let fallbacked = connection
        .eth_proof_manager_dal()
        .fallback_batches(timeout, Duration::ZERO, timeout)
        .await
        .unwrap();
    assert_eq!(fallbacked, 1);

    let processor = ctx.processor(ProvingMode::ProvingNetwork).await;
    let picked = processor.lock_batch_for_proving(timeout).await.unwrap();
    assert_eq!(picked, Some(batch));
}

//! Miscellaneous utils used by multiple components.

use std::{
    future::Future,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_config::configs::chain::L1BatchCommitDataGeneratorMode;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_eth_client::{CallFunctionArgs, ClientError, Error as EthClientError, EthInterface};
use zksync_types::{Address, L1BatchNumber, ProtocolVersionId};

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
        .await?;
    Ok(snapshot_recovery.map_or(L1BatchNumber(0), |recovery| recovery.l1_batch_number + 1))
}

/// Obtains a protocol version projected to be applied for the next L2 block. This is either the version used by the last
/// sealed L2 block, or (if there are no L2 blocks), one referenced in the snapshot recovery record.
pub(crate) async fn pending_protocol_version(
    storage: &mut Connection<'_, Core>,
) -> anyhow::Result<ProtocolVersionId> {
    static WARNED_ABOUT_NO_VERSION: AtomicBool = AtomicBool::new(false);

    let last_l2_block = storage
        .blocks_dal()
        .get_last_sealed_l2_block_header()
        .await?;
    if let Some(last_l2_block) = last_l2_block {
        return Ok(last_l2_block.protocol_version.unwrap_or_else(|| {
            // Protocol version should be set for the most recent L2 block even in cases it's not filled
            // for old L2 blocks, hence the warning. We don't want to rely on this assumption, so we treat
            // the lack of it as in other similar places, replacing with the default value.
            if !WARNED_ABOUT_NO_VERSION.fetch_or(true, Ordering::Relaxed) {
                tracing::warn!("Protocol version not set for recent L2 block: {last_l2_block:?}");
            }
            ProtocolVersionId::last_potentially_undefined()
        }));
    }
    // No L2 blocks in the storage; use snapshot recovery information.
    let snapshot_recovery = storage
        .snapshot_recovery_dal()
        .get_applied_snapshot_status()
        .await?
        .context("storage contains neither L2 blocks, nor snapshot recovery info")?;
    Ok(snapshot_recovery.protocol_version)
}

async fn get_pubdata_pricing_mode(
    diamond_proxy_address: Address,
    eth_client: &dyn EthInterface,
) -> Result<L1BatchCommitDataGeneratorMode, EthClientError> {
    CallFunctionArgs::new("getPubdataPricingMode", ())
        .for_contract(
            diamond_proxy_address,
            &zksync_contracts::hyperchain_contract(),
        )
        .call(eth_client)
        .await
}

pub async fn ensure_l1_batch_commit_data_generation_mode(
    selected_l1_batch_commit_data_generator_mode: L1BatchCommitDataGeneratorMode,
    diamond_proxy_address: Address,
    eth_client: &dyn EthInterface,
) -> anyhow::Result<()> {
    match get_pubdata_pricing_mode(diamond_proxy_address, eth_client).await {
        // Getters contract support getPubdataPricingMode method
        Ok(l1_contract_batch_commitment_mode) => {
            anyhow::ensure!(
                l1_contract_batch_commitment_mode == selected_l1_batch_commit_data_generator_mode,
                "The selected L1BatchCommitDataGeneratorMode ({:?}) does not match the commitment mode used on L1 contract ({:?})",
                selected_l1_batch_commit_data_generator_mode,
                l1_contract_batch_commitment_mode
            );

            Ok(())
        }
        // Getters contract does not support getPubdataPricingMode method.
        // This case is accepted for backwards compatibility with older contracts, but emits a
        // warning in case the wrong contract address was passed by the caller.
        Err(EthClientError::EthereumGateway(err))
            if matches!(err.as_ref(), ClientError::Call(_)) =>
        {
            tracing::warn!("Getters contract does not support getPubdataPricingMode method: {err}");
            Ok(())
        }
        Err(err) => anyhow::bail!(err),
    }
}

#[cfg(test)]
mod tests {
    use std::{mem, sync::Mutex};

    use jsonrpsee::types::ErrorObject;
    use zksync_eth_client::{clients::MockEthereum, ClientError};
    use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
    use zksync_types::{ethabi, U256};
    use zksync_web3_decl::error::EnrichedClientError;

    use super::*;

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
            insert_genesis_batch(&mut storage, &GenesisParams::mock())
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

    fn mock_ethereum(token: ethabi::Token, err: Option<EthClientError>) -> MockEthereum {
        let err_mutex = Mutex::new(err);
        MockEthereum::default().with_fallible_call_handler(move |_, _| {
            let err = mem::take(&mut *err_mutex.lock().unwrap());
            if let Some(err) = err {
                Err(err)
            } else {
                Ok(token.clone())
            }
        })
    }

    fn mock_ethereum_with_legacy_contract() -> MockEthereum {
        let err = ClientError::Call(ErrorObject::owned(3, "execution reverted: F", None::<()>));
        let err = EthClientError::EthereumGateway(EnrichedClientError::new(err, "call"));
        mock_ethereum(ethabi::Token::Uint(U256::zero()), Some(err))
    }

    fn mock_ethereum_with_transport_error() -> MockEthereum {
        let err = ClientError::Transport(anyhow::anyhow!("unreachable"));
        let err = EthClientError::EthereumGateway(EnrichedClientError::new(err, "call"));
        mock_ethereum(ethabi::Token::Uint(U256::zero()), Some(err))
    }

    fn mock_ethereum_with_rollup_contract() -> MockEthereum {
        mock_ethereum(ethabi::Token::Uint(U256::zero()), None)
    }

    fn mock_ethereum_with_validium_contract() -> MockEthereum {
        mock_ethereum(ethabi::Token::Uint(U256::one()), None)
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_succeeds_when_both_match() {
        let addr = Address::repeat_byte(0x01);
        ensure_l1_batch_commit_data_generation_mode(
            L1BatchCommitDataGeneratorMode::Rollup,
            addr,
            &mock_ethereum_with_rollup_contract(),
        )
        .await
        .unwrap();

        ensure_l1_batch_commit_data_generation_mode(
            L1BatchCommitDataGeneratorMode::Validium,
            addr,
            &mock_ethereum_with_validium_contract(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_succeeds_on_legacy_contracts() {
        let addr = Address::repeat_byte(0x01);
        ensure_l1_batch_commit_data_generation_mode(
            L1BatchCommitDataGeneratorMode::Rollup,
            addr,
            &mock_ethereum_with_legacy_contract(),
        )
        .await
        .unwrap();

        ensure_l1_batch_commit_data_generation_mode(
            L1BatchCommitDataGeneratorMode::Validium,
            addr,
            &mock_ethereum_with_legacy_contract(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_fails_on_mismatch() {
        let addr = Address::repeat_byte(0x01);
        let err = ensure_l1_batch_commit_data_generation_mode(
            L1BatchCommitDataGeneratorMode::Validium,
            addr,
            &mock_ethereum_with_rollup_contract(),
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(err.contains("commitment mode"), "{err}");

        let err = ensure_l1_batch_commit_data_generation_mode(
            L1BatchCommitDataGeneratorMode::Rollup,
            addr,
            &mock_ethereum_with_validium_contract(),
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(err.contains("commitment mode"), "{err}");
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_fails_on_request_failure() {
        let addr = Address::repeat_byte(0x01);
        let err = ensure_l1_batch_commit_data_generation_mode(
            L1BatchCommitDataGeneratorMode::Rollup,
            addr,
            &mock_ethereum_with_transport_error(),
        )
        .await
        .unwrap_err();

        assert!(
            err.chain().any(|cause| cause.is::<ClientError>()),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_fails_on_parse_error() {
        let addr = Address::repeat_byte(0x01);

        let err = ensure_l1_batch_commit_data_generation_mode(
            L1BatchCommitDataGeneratorMode::Rollup,
            addr,
            &mock_ethereum(ethabi::Token::String("what".into()), None),
        )
        .await
        .unwrap_err()
        .to_string();

        assert!(
            err.contains("L1BatchCommitDataGeneratorMode::from_tokens"),
            "{err}",
        );
    }
}

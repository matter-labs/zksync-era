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
use zksync_eth_client::{CallFunctionArgs, Error as EthClientError, EthInterface};
use zksync_l1_contract_interface::Detokenize;
use zksync_types::{
    ethabi::{self, Address},
    L1BatchNumber, ProtocolVersionId,
};

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
        tracing::debug!("No L1 batches are present in DB; trying again in {poll_interval:?}");

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

async fn get_pubdata_pricing_mode(
    diamond_proxy_address: Address,
    eth_client: &impl EthInterface,
) -> Result<Vec<ethabi::Token>, EthClientError> {
    let args = CallFunctionArgs::new("getPubdataPricingMode", ())
        .for_contract(diamond_proxy_address, zksync_contracts::zksync_contract());
    eth_client.call_contract_function(args).await
}

pub async fn ensure_l1_batch_commit_data_generation_mode(
    selected_l1_batch_commit_data_generator_mode: L1BatchCommitDataGeneratorMode,
    diamond_proxy_address: Address,
    eth_client: &impl EthInterface,
) -> anyhow::Result<()> {
    match get_pubdata_pricing_mode(diamond_proxy_address, eth_client).await {
        // Getters contract support getPubdataPricingMode method
        Ok(l1_contract_pubdata_pricing_mode) => {
            let l1_contract_batch_commitment_mode =
                L1BatchCommitDataGeneratorMode::from_tokens(l1_contract_pubdata_pricing_mode)
                    .context(
                        "Unable to parse L1BatchCommitDataGeneratorMode received from L1 contract",
                    )?;

            // contracts mode == server mode
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
        Err(EthClientError::Contract(_)) => {
            tracing::warn!("Getters contract does not support getPubdataPricingMode method");
            Ok(())
        }
        Err(err) => anyhow::bail!(err),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use assert_matches::assert_matches;
    use zksync_eth_client::{
        Block, ContractCall, ExecutedTxStatus, FailureInfo, RawTransactionBytes,
    };
    use zksync_types::{
        web3::types::{BlockId, BlockNumber, Filter, Log, Transaction, TransactionReceipt},
        H160, H256, U256, U64,
    };

    use super::*;
    use crate::genesis::{insert_genesis_batch, GenesisParams};

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

    #[derive(Debug)]
    struct MockEthereumForCommitGenerationMode {
        retval: Vec<ethabi::Token>,
        // Can't copy `Error` so use internal mutability to `take` it.
        // This means the the error is one use, reload if you need a second call.
        // We also can't use `RefCell`, since `EthInterface` requires implementors to be `Sync` and
        // `Send`.
        error: Mutex<Option<EthClientError>>,
    }

    impl MockEthereumForCommitGenerationMode {
        fn with_retval(retval: Vec<ethabi::Token>) -> Self {
            Self {
                retval,
                error: Mutex::new(None),
            }
        }

        fn with_error(error: EthClientError) -> Self {
            Self {
                retval: Vec::new(),
                error: Mutex::new(Some(error)),
            }
        }

        fn with_contract_error() -> Self {
            Self::with_error(EthClientError::Contract(
                zksync_types::web3::contract::Error::InterfaceUnsupported,
            ))
        }

        fn with_tx_error() -> Self {
            Self::with_error(EthClientError::EthereumGateway(
                zksync_types::web3::Error::Unreachable,
            ))
        }

        fn with_legacy_contract() -> Self {
            Self::with_contract_error()
        }

        fn with_rollup_contract() -> Self {
            Self::with_retval(vec![ethabi::Token::Uint(U256::zero())])
        }

        fn with_validium_contract() -> Self {
            Self::with_retval(vec![ethabi::Token::Uint(U256::one())])
        }
    }

    #[async_trait]
    impl EthInterface for MockEthereumForCommitGenerationMode {
        async fn get_tx_status(
            &self,
            _: H256,
            _: &'static str,
        ) -> Result<Option<ExecutedTxStatus>, EthClientError> {
            unimplemented!("Not needed");
        }

        async fn send_raw_tx(&self, _: RawTransactionBytes) -> Result<H256, EthClientError> {
            unimplemented!("Not needed");
        }

        async fn nonce_at_for_account(
            &self,
            _: Address,
            _: BlockNumber,
            _: &'static str,
        ) -> Result<U256, EthClientError> {
            unimplemented!("Not needed");
        }

        async fn base_fee_history(
            &self,
            _: usize,
            _: usize,
            _: &'static str,
        ) -> Result<Vec<u64>, EthClientError> {
            unimplemented!("Not needed");
        }

        async fn get_pending_block_base_fee_per_gas(
            &self,
            _: &'static str,
        ) -> Result<U256, EthClientError> {
            unimplemented!("Not needed");
        }

        async fn get_gas_price(&self, _: &'static str) -> Result<U256, EthClientError> {
            unimplemented!("Not needed");
        }

        async fn block_number(&self, _: &'static str) -> Result<U64, EthClientError> {
            unimplemented!("Not needed");
        }

        async fn failure_reason(&self, _: H256) -> Result<Option<FailureInfo>, EthClientError> {
            unimplemented!("Not needed");
        }

        async fn get_tx(
            &self,
            _: H256,
            _: &'static str,
        ) -> Result<Option<Transaction>, EthClientError> {
            unimplemented!("Not needed");
        }

        async fn tx_receipt(
            &self,
            _: H256,
            _: &'static str,
        ) -> Result<Option<TransactionReceipt>, EthClientError> {
            unimplemented!("Not needed");
        }

        async fn eth_balance(&self, _: H160, _: &'static str) -> Result<U256, EthClientError> {
            unimplemented!("Not needed");
        }

        async fn call_contract_function(
            &self,
            _: ContractCall,
        ) -> Result<Vec<ethabi::Token>, EthClientError> {
            let mut error = None;
            core::mem::swap(&mut *self.error.lock().unwrap(), &mut error);
            if let Some(error) = error {
                return Err(error);
            }
            Ok(self.retval.clone())
        }

        async fn logs(&self, _: Filter, _: &'static str) -> Result<Vec<Log>, EthClientError> {
            unimplemented!("Not needed");
        }

        async fn block(
            &self,
            _: BlockId,
            _: &'static str,
        ) -> Result<Option<Block<H256>>, EthClientError> {
            unimplemented!("Not needed");
        }
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_succeeds_when_both_match() {
        let addr = Address::repeat_byte(0x01);
        assert_matches!(
            ensure_l1_batch_commit_data_generation_mode(
                L1BatchCommitDataGeneratorMode::Rollup,
                addr,
                &MockEthereumForCommitGenerationMode::with_rollup_contract(),
            )
            .await,
            Ok(())
        );
        assert_matches!(
            ensure_l1_batch_commit_data_generation_mode(
                L1BatchCommitDataGeneratorMode::Validium,
                addr,
                &MockEthereumForCommitGenerationMode::with_validium_contract(),
            )
            .await,
            Ok(())
        );
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_succeeds_on_legacy_contracts() {
        let addr = Address::repeat_byte(0x01);
        assert_matches!(
            ensure_l1_batch_commit_data_generation_mode(
                L1BatchCommitDataGeneratorMode::Rollup,
                addr,
                &MockEthereumForCommitGenerationMode::with_legacy_contract(),
            )
            .await,
            Ok(())
        );
        assert_matches!(
            ensure_l1_batch_commit_data_generation_mode(
                L1BatchCommitDataGeneratorMode::Validium,
                addr,
                &MockEthereumForCommitGenerationMode::with_legacy_contract(),
            )
            .await,
            Ok(())
        );
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_fails_on_mismatch() {
        let addr = Address::repeat_byte(0x01);
        assert_eq!(
            ensure_l1_batch_commit_data_generation_mode(
                L1BatchCommitDataGeneratorMode::Validium,
                addr,
                &MockEthereumForCommitGenerationMode::with_rollup_contract(),
            ).await.unwrap_err().to_string(),
            "The selected L1BatchCommitDataGeneratorMode (Validium) does not match the commitment mode used on L1 contract (Rollup)",
        );
        assert_eq!(
            ensure_l1_batch_commit_data_generation_mode(
                L1BatchCommitDataGeneratorMode::Rollup,
                addr,
                &MockEthereumForCommitGenerationMode::with_validium_contract(),
            ).await.unwrap_err().to_string(),
            "The selected L1BatchCommitDataGeneratorMode (Rollup) does not match the commitment mode used on L1 contract (Validium)",
        );
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_fails_on_request_failure() {
        let addr = Address::repeat_byte(0x01);
        assert_eq!(
            ensure_l1_batch_commit_data_generation_mode(
                L1BatchCommitDataGeneratorMode::Rollup,
                addr,
                &MockEthereumForCommitGenerationMode::with_tx_error(),
            )
            .await
            .unwrap_err()
            .to_string(),
            "Request to ethereum gateway failed: Server is unreachable",
        );
        assert_eq!(
            ensure_l1_batch_commit_data_generation_mode(
                L1BatchCommitDataGeneratorMode::Validium,
                addr,
                &MockEthereumForCommitGenerationMode::with_tx_error(),
            )
            .await
            .unwrap_err()
            .to_string(),
            "Request to ethereum gateway failed: Server is unreachable",
        );
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_fails_on_parse_error() {
        let addr = Address::repeat_byte(0x01);
        assert_eq!(
            ensure_l1_batch_commit_data_generation_mode(
                L1BatchCommitDataGeneratorMode::Rollup,
                addr,
                &MockEthereumForCommitGenerationMode::with_retval(vec![]),
            )
            .await
            .unwrap_err()
            .to_string(),
            "Unable to parse L1BatchCommitDataGeneratorMode received from L1 contract",
        );
        assert_eq!(
            ensure_l1_batch_commit_data_generation_mode(
                L1BatchCommitDataGeneratorMode::Validium,
                addr,
                &MockEthereumForCommitGenerationMode::with_retval(vec![]),
            )
            .await
            .unwrap_err()
            .to_string(),
            "Unable to parse L1BatchCommitDataGeneratorMode received from L1 contract",
        );
    }
}

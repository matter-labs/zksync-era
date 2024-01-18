use std::time::{Duration, Instant};

use anyhow::Context;
use multivm::{
    interface::{L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode},
    vm_latest::constants::BLOCK_GAS_LIMIT,
};
use zksync_contracts::BaseSystemContracts;
use zksync_dal::StorageProcessor;
use zksync_types::{
    block::MiniblockHeader, fee_model::BatchFeeInput, snapshots::SnapshotRecoveryStatus, Address,
    L1BatchNumber, L2ChainId, MiniblockNumber, ProtocolVersionId, H256, ZKPORTER_IS_AVAILABLE,
};

use super::PendingBatchData;
use crate::state_keeper::extractors;

/// Returns the parameters required to initialize the VM for the next L1 batch.
#[allow(clippy::too_many_arguments)]
pub(crate) fn l1_batch_params(
    current_l1_batch_number: L1BatchNumber,
    fee_account: Address,
    l1_batch_timestamp: u64,
    previous_batch_hash: H256,
    fee_input: BatchFeeInput,
    first_miniblock_number: MiniblockNumber,
    prev_miniblock_hash: H256,
    base_system_contracts: BaseSystemContracts,
    validation_computational_gas_limit: u32,
    protocol_version: ProtocolVersionId,
    virtual_blocks: u32,
    chain_id: L2ChainId,
) -> (SystemEnv, L1BatchEnv) {
    (
        SystemEnv {
            zk_porter_available: ZKPORTER_IS_AVAILABLE,
            version: protocol_version,
            base_system_smart_contracts: base_system_contracts,
            gas_limit: BLOCK_GAS_LIMIT,
            execution_mode: TxExecutionMode::VerifyExecute,
            default_validation_computational_gas_limit: validation_computational_gas_limit,
            chain_id,
        },
        L1BatchEnv {
            previous_batch_hash: Some(previous_batch_hash),
            number: current_l1_batch_number,
            timestamp: l1_batch_timestamp,
            fee_input,
            fee_account,
            enforced_base_fee: None,
            first_l2_block: L2BlockEnv {
                number: first_miniblock_number.0,
                timestamp: l1_batch_timestamp,
                prev_block_hash: prev_miniblock_hash,
                max_virtual_blocks_to_create: virtual_blocks,
            },
        },
    )
}

/// Returns the amount of iterations `delay_interval` fits into `max_wait`, rounding up.
pub(crate) fn poll_iters(delay_interval: Duration, max_wait: Duration) -> usize {
    let max_wait_millis = max_wait.as_millis() as u64;
    let delay_interval_millis = delay_interval.as_millis() as u64;
    assert!(delay_interval_millis > 0, "delay interval must be positive");

    ((max_wait_millis + delay_interval_millis - 1) / delay_interval_millis).max(1) as usize
}

/// Cursor of the miniblock / L1 batch progress used by [`StateKeeperIO`](super::StateKeeperIO) implementations.
#[derive(Debug)]
pub(crate) struct IoCursor {
    pub next_miniblock: MiniblockNumber,
    pub prev_miniblock_hash: H256,
    pub prev_miniblock_timestamp: u64,
    pub l1_batch: L1BatchNumber,
}

impl IoCursor {
    /// Loads the cursor from Postgres.
    pub async fn new(storage: &mut StorageProcessor<'_>) -> anyhow::Result<Self> {
        let last_sealed_l1_batch_number = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .context("Failed getting sealed L1 batch number")?;
        let last_miniblock_header = storage
            .blocks_dal()
            .get_last_sealed_miniblock_header()
            .await
            .context("Failed getting sealed miniblock header")?;

        if let (Some(l1_batch_number), Some(miniblock_header)) =
            (last_sealed_l1_batch_number, &last_miniblock_header)
        {
            Ok(Self {
                next_miniblock: miniblock_header.number + 1,
                prev_miniblock_hash: miniblock_header.hash,
                prev_miniblock_timestamp: miniblock_header.timestamp,
                l1_batch: l1_batch_number + 1,
            })
        } else {
            let snapshot_recovery = storage
                .snapshot_recovery_dal()
                .get_applied_snapshot_status()
                .await
                .context("Failed getting snapshot recovery info")?
                .context("Postgres contains neither blocks nor snapshot recovery info")?;
            let l1_batch =
                last_sealed_l1_batch_number.unwrap_or(snapshot_recovery.l1_batch_number) + 1;

            let (next_miniblock, prev_miniblock_hash, prev_miniblock_timestamp);
            if let Some(miniblock_header) = &last_miniblock_header {
                next_miniblock = miniblock_header.number + 1;
                prev_miniblock_hash = miniblock_header.hash;
                prev_miniblock_timestamp = miniblock_header.timestamp;
            } else {
                next_miniblock = snapshot_recovery.miniblock_number + 1;
                prev_miniblock_hash = snapshot_recovery.miniblock_hash;
                prev_miniblock_timestamp = snapshot_recovery.miniblock_timestamp;
            }

            Ok(Self {
                next_miniblock,
                prev_miniblock_hash,
                prev_miniblock_timestamp,
                l1_batch,
            })
        }
    }
}

/// Typesafe wrapper around [`MiniblockHeader`] returned by [`L1BatchParamsProvider`].
#[derive(Debug)]
pub(crate) struct FirstMiniblockInBatch {
    header: MiniblockHeader,
    l1_batch_number: L1BatchNumber,
}

impl FirstMiniblockInBatch {
    pub fn number(&self) -> MiniblockNumber {
        self.header.number
    }

    pub fn has_protocol_version(&self) -> bool {
        self.header.protocol_version.is_some()
    }

    pub fn set_protocol_version(&mut self, version: ProtocolVersionId) {
        assert!(
            self.header.protocol_version.is_none(),
            "Cannot redefine protocol version"
        );
        self.header.protocol_version = Some(version);
    }
}

/// Provider of L1 batch parameters for state keeper I/O implementations. The provider is stateless; i.e., it doesn't
/// enforce a particular order of method calls.
#[derive(Debug)]
pub(crate) struct L1BatchParamsProvider {
    snapshot: Option<SnapshotRecoveryStatus>,
}

impl L1BatchParamsProvider {
    pub async fn new(storage: &mut StorageProcessor<'_>) -> anyhow::Result<Self> {
        let snapshot = storage
            .snapshot_recovery_dal()
            .get_applied_snapshot_status()
            .await?;
        Ok(Self { snapshot })
    }

    /// Returns state root hash and timestamp of an L1 batch with the specified number waiting for the hash to be computed
    /// if necessary.
    pub async fn wait_for_l1_batch_params(
        &self,
        storage: &mut StorageProcessor<'_>,
        number: L1BatchNumber,
    ) -> anyhow::Result<(H256, u64)> {
        let first_l1_batch = if let Some(snapshot) = &self.snapshot {
            // Special case: if we've recovered from a snapshot, we allow to wait for the snapshot L1 batch.
            if number == snapshot.l1_batch_number {
                return Ok((snapshot.l1_batch_root_hash, snapshot.l1_batch_timestamp));
            }
            snapshot.l1_batch_number + 1
        } else {
            L1BatchNumber(0)
        };

        anyhow::ensure!(
            number >= first_l1_batch,
            "Cannot wait a hash of a pruned L1 batch #{number} (first retained batch: {first_l1_batch})"
        );
        Self::wait_for_l1_batch_params_unchecked(storage, number).await
    }

    async fn wait_for_l1_batch_params_unchecked(
        storage: &mut StorageProcessor<'_>,
        number: L1BatchNumber,
    ) -> anyhow::Result<(H256, u64)> {
        // If the state root is not known yet, this duration will be used to back off in the while loops
        const SAFE_STATE_ROOT_INTERVAL: Duration = Duration::from_millis(100);

        let stage_started_at: Instant = Instant::now();
        loop {
            let data = storage
                .blocks_dal()
                .get_l1_batch_state_root_and_timestamp(number)
                .await?;
            if let Some((root_hash, timestamp)) = data {
                tracing::trace!(
                    "Waiting for hash of L1 batch #{number} took {:?}",
                    stage_started_at.elapsed()
                );
                return Ok((root_hash, timestamp));
            }

            tokio::time::sleep(SAFE_STATE_ROOT_INTERVAL).await;
        }
    }

    /// Returns a header of the first miniblock in the specified L1 batch regardless of whether the batch is sealed or not.
    pub(crate) async fn load_first_miniblock_in_batch(
        &self,
        storage: &mut StorageProcessor<'_>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<FirstMiniblockInBatch>> {
        let miniblock_number = self
            .load_number_of_first_miniblock_in_batch(storage, l1_batch_number)
            .await
            .context("failed getting first miniblock number")?;
        Ok(match miniblock_number {
            Some(number) => storage
                .blocks_dal()
                .get_miniblock_header(number)
                .await
                .context("failed getting miniblock header")?
                .map(|header| FirstMiniblockInBatch {
                    header,
                    l1_batch_number,
                }),
            None => None,
        })
    }

    async fn load_number_of_first_miniblock_in_batch(
        &self,
        storage: &mut StorageProcessor<'_>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<MiniblockNumber>> {
        if l1_batch_number == L1BatchNumber(0) {
            return Ok(Some(MiniblockNumber(0)));
        }

        if let Some(snapshot) = &self.snapshot {
            anyhow::ensure!(
                l1_batch_number > snapshot.l1_batch_number,
                "Cannot load miniblocks for pruned L1 batch #{l1_batch_number} (first retained batch: {})",
                snapshot.l1_batch_number + 1
            );
            if l1_batch_number == snapshot.l1_batch_number + 1 {
                return Ok(Some(snapshot.miniblock_number + 1));
            }
        }

        let prev_l1_batch = l1_batch_number - 1;
        // At this point, we have ensured that `prev_l1_batch` is not pruned.
        let Some((_, last_miniblock_in_prev_l1_batch)) = storage
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(prev_l1_batch)
            .await
            .with_context(|| {
                format!("failed getting miniblock range for L1 batch #{prev_l1_batch}")
            })?
        else {
            return Ok(None);
        };
        Ok(Some(last_miniblock_in_prev_l1_batch + 1))
    }

    /// Loads VM-related L1 batch parameters for the specified batch.
    pub(crate) async fn load_l1_batch_params(
        &self,
        storage: &mut StorageProcessor<'_>,
        first_miniblock_in_batch: &FirstMiniblockInBatch,
        fee_account: Address,
        validation_computational_gas_limit: u32,
        chain_id: L2ChainId,
    ) -> anyhow::Result<(SystemEnv, L1BatchEnv)> {
        anyhow::ensure!(
            first_miniblock_in_batch.l1_batch_number > L1BatchNumber(0),
            "Loading params for genesis L1 batch not supported"
        );
        // L1 batch timestamp is set to the timestamp of its first miniblock.
        let l1_batch_timestamp = first_miniblock_in_batch.header.timestamp;

        let prev_l1_batch_number = first_miniblock_in_batch.l1_batch_number - 1;
        tracing::info!("Getting previous L1 batch hash for batch #{prev_l1_batch_number}");
        let (prev_l1_batch_hash, prev_l1_batch_timestamp) = self
            .wait_for_l1_batch_params(storage, prev_l1_batch_number)
            .await
            .context("failed getting hash for previous L1 batch")?;
        tracing::info!("Got state root hash for previous L1 batch #{prev_l1_batch_number}: {prev_l1_batch_hash:?}");

        anyhow::ensure!(
            prev_l1_batch_timestamp < l1_batch_timestamp,
            "Invalid params for L1 batch #{}: Timestamp of previous L1 batch ({}) >= provisional L1 batch timestamp ({}), \
             meaning that L1 batch will be rejected by the bootloader",
            first_miniblock_in_batch.l1_batch_number,
            extractors::display_timestamp(prev_l1_batch_timestamp),
            extractors::display_timestamp(l1_batch_timestamp)
        );

        let prev_miniblock_number = first_miniblock_in_batch.header.number - 1;
        tracing::info!("Getting previous miniblock hash for miniblock #{prev_miniblock_number}");

        let prev_miniblock_hash = self.snapshot.as_ref().and_then(|snapshot| {
            (snapshot.miniblock_number == prev_miniblock_number).then_some(snapshot.miniblock_hash)
        });
        let prev_miniblock_hash = match prev_miniblock_hash {
            Some(hash) => hash,
            None => storage
                .blocks_web3_dal()
                .get_miniblock_hash(prev_miniblock_number)
                .await
                .context("failed getting hash for previous miniblock")?
                .context("previous miniblock disappeared from storage")?,
        };
        tracing::info!(
            "Got hash for previous miniblock #{prev_miniblock_number}: {prev_miniblock_hash:?}"
        );

        let contract_hashes = first_miniblock_in_batch.header.base_system_contracts_hashes;
        let base_system_contracts = storage
            .storage_dal()
            .get_base_system_contracts(contract_hashes.bootloader, contract_hashes.default_aa)
            .await;

        Ok(l1_batch_params(
            first_miniblock_in_batch.l1_batch_number,
            fee_account,
            l1_batch_timestamp,
            prev_l1_batch_hash,
            first_miniblock_in_batch.header.batch_fee_input,
            first_miniblock_in_batch.header.number,
            prev_miniblock_hash,
            base_system_contracts,
            validation_computational_gas_limit,
            first_miniblock_in_batch
                .header
                .protocol_version
                .context("`protocol_version` must be set for miniblock")?,
            first_miniblock_in_batch.header.virtual_blocks,
            chain_id,
        ))
    }

    /// Loads the pending L1 batch data from the database.
    ///
    /// # Errors
    ///
    /// Propagates DB errors. Also returns an error if `first_miniblock_in_batch` doesn't correspond to a pending L1 batch.
    pub(crate) async fn load_pending_batch(
        &self,
        storage: &mut StorageProcessor<'_>,
        first_miniblock_in_batch: &FirstMiniblockInBatch,
        fee_account: Address,
        validation_computational_gas_limit: u32,
        chain_id: L2ChainId,
    ) -> anyhow::Result<PendingBatchData> {
        let (system_env, l1_batch_env) = self
            .load_l1_batch_params(
                storage,
                first_miniblock_in_batch,
                fee_account,
                validation_computational_gas_limit,
                chain_id,
            )
            .await
            .context("failed loading L1 batch params")?;

        let pending_miniblocks = storage
            .transactions_dal()
            .get_miniblocks_to_reexecute()
            .await
            .context("failed loading miniblocks for re-execution")?;
        let first_pending_miniblock = pending_miniblocks
            .first()
            .context("no pending miniblocks; was `first_miniblock_in_batch` loaded for a correct L1 batch number?")?;
        anyhow::ensure!(
            first_pending_miniblock.number == first_miniblock_in_batch.header.number,
            "Invalid `first_miniblock_in_batch` supplied: its L1 batch #{} is not pending",
            first_miniblock_in_batch.l1_batch_number
        );
        Ok(PendingBatchData {
            l1_batch_env,
            system_env,
            pending_miniblocks,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, ops};

    use futures::FutureExt;
    use zksync_contracts::BaseSystemContractsHashes;
    use zksync_dal::ConnectionPool;
    use zksync_types::{
        block::{BlockGasCount, MiniblockHasher},
        fee::TransactionExecutionMetrics,
    };

    use super::*;
    use crate::{
        genesis::{ensure_genesis_state, GenesisParams},
        utils::testonly::{
            create_l1_batch, create_l2_transaction, create_miniblock, execute_l2_transaction,
            prepare_empty_recovery_snapshot,
        },
    };

    #[test]
    #[rustfmt::skip] // One-line formatting looks better here.
    fn test_poll_iters() {
        assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(0)), 1);
        assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(100)), 1);
        assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(101)), 2);
        assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(200)), 2);
        assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(201)), 3);
    }

    #[tokio::test]
    async fn creating_io_cursor_with_genesis() {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.access_storage().await.unwrap();
        ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
            .await
            .unwrap();

        let cursor = IoCursor::new(&mut storage).await.unwrap();
        assert_eq!(cursor.l1_batch, L1BatchNumber(1));
        assert_eq!(cursor.next_miniblock, MiniblockNumber(1));
        assert_eq!(cursor.prev_miniblock_timestamp, 0);
        assert_eq!(
            cursor.prev_miniblock_hash,
            MiniblockHasher::legacy_hash(MiniblockNumber(0))
        );

        let miniblock = create_miniblock(1);
        storage
            .blocks_dal()
            .insert_miniblock(&miniblock)
            .await
            .unwrap();

        let cursor = IoCursor::new(&mut storage).await.unwrap();
        assert_eq!(cursor.l1_batch, L1BatchNumber(1));
        assert_eq!(cursor.next_miniblock, MiniblockNumber(2));
        assert_eq!(cursor.prev_miniblock_timestamp, miniblock.timestamp);
        assert_eq!(cursor.prev_miniblock_hash, miniblock.hash);
    }

    #[tokio::test]
    async fn creating_io_cursor_with_snapshot_recovery() {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.access_storage().await.unwrap();
        let snapshot_recovery = prepare_empty_recovery_snapshot(&mut storage, 23).await;

        let cursor = IoCursor::new(&mut storage).await.unwrap();
        assert_eq!(cursor.l1_batch, L1BatchNumber(24));
        assert_eq!(
            cursor.next_miniblock,
            snapshot_recovery.miniblock_number + 1
        );
        assert_eq!(
            cursor.prev_miniblock_timestamp,
            snapshot_recovery.miniblock_timestamp
        );
        assert_eq!(cursor.prev_miniblock_hash, snapshot_recovery.miniblock_hash);

        // Add a miniblock so that we have miniblocks (but not an L1 batch) in the storage.
        let miniblock = create_miniblock(snapshot_recovery.miniblock_number.0 + 1);
        storage
            .blocks_dal()
            .insert_miniblock(&miniblock)
            .await
            .unwrap();

        let cursor = IoCursor::new(&mut storage).await.unwrap();
        assert_eq!(cursor.l1_batch, L1BatchNumber(24));
        assert_eq!(cursor.next_miniblock, miniblock.number + 1);
        assert_eq!(cursor.prev_miniblock_timestamp, miniblock.timestamp);
        assert_eq!(cursor.prev_miniblock_hash, miniblock.hash);
    }

    #[tokio::test]
    async fn waiting_for_l1_batch_params_with_genesis() {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.access_storage().await.unwrap();
        let genesis_root_hash =
            ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
                .await
                .unwrap();

        let provider = L1BatchParamsProvider::new(&mut storage).await.unwrap();
        assert!(provider.snapshot.is_none());
        let (hash, timestamp) = provider
            .wait_for_l1_batch_params(&mut storage, L1BatchNumber(0))
            .await
            .unwrap();
        assert_eq!(hash, genesis_root_hash);
        assert_eq!(timestamp, 0);

        let new_l1_batch = create_l1_batch(1);
        storage
            .blocks_dal()
            .insert_l1_batch(&new_l1_batch, &[], BlockGasCount::default(), &[], &[], 0)
            .await
            .unwrap();

        let wait_future = provider.wait_for_l1_batch_params(&mut storage, L1BatchNumber(1));
        futures::pin_mut!(wait_future);
        tokio::task::yield_now().await;
        assert!((&mut wait_future).now_or_never().is_none());

        let expected_hash = H256::repeat_byte(1);
        let mut storage = pool.access_storage().await.unwrap();
        storage
            .blocks_dal()
            .set_l1_batch_hash(L1BatchNumber(1), expected_hash)
            .await
            .unwrap();
        let (hash, timestamp) = wait_future.await.unwrap();
        assert_eq!(hash, expected_hash);
        assert_eq!(timestamp, new_l1_batch.timestamp);
    }

    #[tokio::test]
    async fn waiting_for_l1_batch_params_after_snapshot_recovery() {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.access_storage().await.unwrap();
        let snapshot_recovery = prepare_empty_recovery_snapshot(&mut storage, 23).await;

        let provider = L1BatchParamsProvider::new(&mut storage).await.unwrap();
        assert!(provider.snapshot.is_some());
        let (hash, timestamp) = provider
            .wait_for_l1_batch_params(&mut storage, snapshot_recovery.l1_batch_number)
            .await
            .unwrap();
        assert_eq!(hash, snapshot_recovery.l1_batch_root_hash);
        assert_eq!(timestamp, snapshot_recovery.l1_batch_timestamp);

        for pruned_l1_batch in [0, 1, snapshot_recovery.l1_batch_number.0 - 1] {
            assert!(provider
                .wait_for_l1_batch_params(&mut storage, L1BatchNumber(pruned_l1_batch))
                .await
                .is_err());
        }

        let new_l1_batch = create_l1_batch(snapshot_recovery.l1_batch_number.0 + 1);
        storage
            .blocks_dal()
            .insert_l1_batch(&new_l1_batch, &[], BlockGasCount::default(), &[], &[], 0)
            .await
            .unwrap();

        let wait_future =
            provider.wait_for_l1_batch_params(&mut storage, snapshot_recovery.l1_batch_number + 1);
        futures::pin_mut!(wait_future);
        tokio::task::yield_now().await;
        assert!((&mut wait_future).now_or_never().is_none());

        let expected_hash = H256::repeat_byte(1);
        let mut storage = pool.access_storage().await.unwrap();
        storage
            .blocks_dal()
            .set_l1_batch_hash(new_l1_batch.number, expected_hash)
            .await
            .unwrap();
        let (hash, timestamp) = wait_future.await.unwrap();
        assert_eq!(hash, expected_hash);
        assert_eq!(timestamp, new_l1_batch.timestamp);
    }

    #[tokio::test]
    async fn getting_first_miniblock_in_batch_with_genesis() {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.access_storage().await.unwrap();
        ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
            .await
            .unwrap();

        let provider = L1BatchParamsProvider::new(&mut storage).await.unwrap();
        let mut batches_and_miniblocks = HashMap::from([
            (L1BatchNumber(0), Ok(Some(MiniblockNumber(0)))),
            (L1BatchNumber(1), Ok(Some(MiniblockNumber(1)))),
            (L1BatchNumber(2), Ok(None)),
            (L1BatchNumber(100), Ok(None)),
        ]);
        assert_first_miniblock_numbers(&provider, &mut storage, &batches_and_miniblocks).await;

        let new_miniblock = create_miniblock(1);
        storage
            .blocks_dal()
            .insert_miniblock(&new_miniblock)
            .await
            .unwrap();
        let new_miniblock = create_miniblock(2);
        storage
            .blocks_dal()
            .insert_miniblock(&new_miniblock)
            .await
            .unwrap();

        assert_first_miniblock_numbers(&provider, &mut storage, &batches_and_miniblocks).await;

        let new_l1_batch = create_l1_batch(1);
        storage
            .blocks_dal()
            .insert_l1_batch(&new_l1_batch, &[], BlockGasCount::default(), &[], &[], 0)
            .await
            .unwrap();
        storage
            .blocks_dal()
            .mark_miniblocks_as_executed_in_l1_batch(new_l1_batch.number)
            .await
            .unwrap();

        batches_and_miniblocks.insert(L1BatchNumber(2), Ok(Some(MiniblockNumber(3))));
        assert_first_miniblock_numbers(&provider, &mut storage, &batches_and_miniblocks).await;
    }

    async fn assert_first_miniblock_numbers(
        provider: &L1BatchParamsProvider,
        storage: &mut StorageProcessor<'_>,
        batches_and_miniblocks: &HashMap<L1BatchNumber, Result<Option<MiniblockNumber>, ()>>,
    ) {
        for (&batch, &expected_miniblock) in batches_and_miniblocks {
            let number = provider
                .load_number_of_first_miniblock_in_batch(storage, batch)
                .await;
            match expected_miniblock {
                Ok(expected) => {
                    assert_eq!(
                        number.unwrap(),
                        expected,
                        "load_number_of_first_miniblock_in_batch({batch})"
                    );
                }
                Err(()) => {
                    number.unwrap_err();
                }
            }
        }
    }

    #[tokio::test]
    async fn getting_first_miniblock_in_batch_after_snapshot_recovery() {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.access_storage().await.unwrap();
        let snapshot_recovery = prepare_empty_recovery_snapshot(&mut storage, 23).await;

        let provider = L1BatchParamsProvider::new(&mut storage).await.unwrap();
        let mut batches_and_miniblocks = HashMap::from([
            (L1BatchNumber(1), Err(())),
            (snapshot_recovery.l1_batch_number, Err(())),
            (
                snapshot_recovery.l1_batch_number + 1,
                Ok(Some(snapshot_recovery.miniblock_number + 1)),
            ),
            (snapshot_recovery.l1_batch_number + 2, Ok(None)),
            (L1BatchNumber(100), Ok(None)),
        ]);
        assert_first_miniblock_numbers(&provider, &mut storage, &batches_and_miniblocks).await;

        let new_miniblock = create_miniblock(snapshot_recovery.miniblock_number.0 + 1);
        storage
            .blocks_dal()
            .insert_miniblock(&new_miniblock)
            .await
            .unwrap();

        assert_first_miniblock_numbers(&provider, &mut storage, &batches_and_miniblocks).await;

        let new_l1_batch = create_l1_batch(snapshot_recovery.l1_batch_number.0 + 1);
        storage
            .blocks_dal()
            .insert_l1_batch(&new_l1_batch, &[], BlockGasCount::default(), &[], &[], 0)
            .await
            .unwrap();
        storage
            .blocks_dal()
            .mark_miniblocks_as_executed_in_l1_batch(new_l1_batch.number)
            .await
            .unwrap();

        batches_and_miniblocks.insert(
            snapshot_recovery.l1_batch_number + 2,
            Ok(Some(new_miniblock.number + 1)),
        );
        assert_first_miniblock_numbers(&provider, &mut storage, &batches_and_miniblocks).await;
    }

    #[tokio::test]
    async fn loading_pending_batch_with_genesis() {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.access_storage().await.unwrap();
        let genesis_params = GenesisParams::mock();
        ensure_genesis_state(&mut storage, L2ChainId::default(), &genesis_params)
            .await
            .unwrap();
        store_pending_miniblocks(
            &mut storage,
            1..=2,
            genesis_params.base_system_contracts.hashes(),
        )
        .await;

        let provider = L1BatchParamsProvider::new(&mut storage).await.unwrap();
        let first_miniblock_in_batch = provider
            .load_first_miniblock_in_batch(&mut storage, L1BatchNumber(1))
            .await
            .unwrap()
            .expect("no first miniblock");
        assert_eq!(first_miniblock_in_batch.number(), MiniblockNumber(1));

        let pending_batch = provider
            .load_pending_batch(
                &mut storage,
                &first_miniblock_in_batch,
                Address::zero(),
                u32::MAX,
                L2ChainId::default(),
            )
            .await
            .unwrap();

        assert_eq!(pending_batch.pending_miniblocks.len(), 2);
        assert_eq!(pending_batch.l1_batch_env.number, L1BatchNumber(1));
        assert_eq!(pending_batch.l1_batch_env.timestamp, 1);
        assert_eq!(pending_batch.l1_batch_env.first_l2_block.number, 1);
        assert_eq!(pending_batch.l1_batch_env.first_l2_block.timestamp, 1);
        assert_eq!(
            pending_batch.l1_batch_env.first_l2_block.prev_block_hash,
            MiniblockHasher::legacy_hash(MiniblockNumber(0))
        );
    }

    async fn store_pending_miniblocks(
        storage: &mut StorageProcessor<'_>,
        numbers: ops::RangeInclusive<u32>,
        contract_hashes: BaseSystemContractsHashes,
    ) {
        for miniblock_number in numbers {
            let tx = create_l2_transaction(10, 100);
            storage
                .transactions_dal()
                .insert_transaction_l2(tx.clone(), TransactionExecutionMetrics::default())
                .await;
            let mut new_miniblock = create_miniblock(miniblock_number);
            new_miniblock.base_system_contracts_hashes = contract_hashes;
            storage
                .blocks_dal()
                .insert_miniblock(&new_miniblock)
                .await
                .unwrap();
            let tx_result = execute_l2_transaction(tx);
            storage
                .transactions_dal()
                .mark_txs_as_executed_in_miniblock(new_miniblock.number, &[tx_result], 1.into())
                .await;
        }
    }

    #[tokio::test]
    async fn loading_pending_batch_after_snapshot_recovery() {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.access_storage().await.unwrap();
        let snapshot_recovery = prepare_empty_recovery_snapshot(&mut storage, 23).await;
        let contracts = GenesisParams::mock().base_system_contracts;
        let factory_deps = HashMap::from([
            (
                contracts.bootloader.hash,
                zksync_utils::be_words_to_bytes(&contracts.bootloader.code),
            ),
            (
                contracts.default_aa.hash,
                zksync_utils::be_words_to_bytes(&contracts.default_aa.code),
            ),
        ]);

        let starting_miniblock_number = snapshot_recovery.miniblock_number.0 + 1;
        store_pending_miniblocks(
            &mut storage,
            starting_miniblock_number..=starting_miniblock_number + 1,
            contracts.hashes(),
        )
        .await;
        storage
            .storage_dal()
            .insert_factory_deps(snapshot_recovery.miniblock_number + 1, &factory_deps)
            .await;

        let provider = L1BatchParamsProvider::new(&mut storage).await.unwrap();
        let first_miniblock_in_batch = provider
            .load_first_miniblock_in_batch(&mut storage, snapshot_recovery.l1_batch_number + 1)
            .await
            .unwrap()
            .expect("no first miniblock");
        assert_eq!(
            first_miniblock_in_batch.number(),
            snapshot_recovery.miniblock_number + 1
        );

        let pending_batch = provider
            .load_pending_batch(
                &mut storage,
                &first_miniblock_in_batch,
                Address::zero(),
                u32::MAX,
                L2ChainId::default(),
            )
            .await
            .unwrap();

        let expected_timestamp = u64::from(snapshot_recovery.miniblock_number.0) + 1;
        assert_eq!(pending_batch.pending_miniblocks.len(), 2);
        assert_eq!(
            pending_batch.l1_batch_env.number,
            snapshot_recovery.l1_batch_number + 1
        );
        assert_eq!(pending_batch.l1_batch_env.timestamp, expected_timestamp);
        assert_eq!(
            pending_batch.l1_batch_env.first_l2_block.number,
            snapshot_recovery.miniblock_number.0 + 1
        );
        assert_eq!(
            pending_batch.l1_batch_env.first_l2_block.timestamp,
            expected_timestamp
        );
        assert_eq!(
            pending_batch.l1_batch_env.first_l2_block.prev_block_hash,
            snapshot_recovery.miniblock_hash
        );
    }
}

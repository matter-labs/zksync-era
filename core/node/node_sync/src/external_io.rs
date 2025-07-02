use std::time::{Duration, Instant};

use anyhow::Context as _;
use async_trait::async_trait;
use zksync_contracts::{BaseSystemContracts, BaseSystemContractsHashes};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_state_keeper::{
    io::{
        common::{load_pending_batch, IoCursor},
        seal_logic::l2_block_seal_subtasks::L2BlockSealProcess,
        IOOpenBatch, L1BatchParams, L2BlockParams, PendingBatchData, StateKeeperIO,
    },
    metrics::KEEPER_METRICS,
    seal_criteria::{IoSealCriteria, UnexecutableReason},
    updates::UpdatesManager,
};
use zksync_types::{
    block::UnsealedL1BatchHeader,
    protocol_upgrade::ProtocolUpgradeTx,
    protocol_version::{ProtocolSemanticVersion, VersionPatch},
    L1BatchNumber, L2ChainId, ProtocolVersionId, Transaction, H256,
};
use zksync_vm_executor::storage::{get_base_system_contracts_by_version_id, L1BatchParamsProvider};

use super::{
    client::MainNodeClient,
    sync_action::{ActionQueue, SyncAction},
};

/// ExternalIO is the IO abstraction for the state keeper that is used in the external node.
///
/// It receives a sequence of actions from the fetcher via the action queue and propagates it
/// into the state keeper.
///
/// It is also responsible for the persisting of data, and this slice of logic is pretty close
/// to the one in the mempool IO (which is used in the main node).
#[derive(Debug)]
pub struct ExternalIO {
    pool: ConnectionPool<Core>,
    l1_batch_params_provider: L1BatchParamsProvider,
    actions: ActionQueue,
    // Main node client is only used for non-leader node to fetch protocol upgrade data.
    // It must not be set if node is a leader.
    main_node_client: Option<Box<dyn MainNodeClient>>,
    chain_id: L2ChainId,
    open_batch: Option<IOOpenBatch>,
}

impl ExternalIO {
    pub fn new(
        pool: ConnectionPool<Core>,
        actions: ActionQueue,
        main_node_client: Option<Box<dyn MainNodeClient>>,
        chain_id: L2ChainId,
    ) -> anyhow::Result<Self> {
        let l1_batch_params_provider = L1BatchParamsProvider::uninitialized();
        Ok(Self {
            pool,
            l1_batch_params_provider,
            actions,
            main_node_client,
            chain_id,
            open_batch: None,
        })
    }

    async fn is_protocol_version_saved(
        &self,
        protocol_version: ProtocolVersionId,
        deadline: Instant,
    ) -> anyhow::Result<bool> {
        let base_system_contract_hashes = self
            .pool
            .connection_tagged("sync_layer")
            .await?
            .protocol_versions_dal()
            .get_base_system_contract_hashes_by_version_id(protocol_version)
            .await?;
        if base_system_contract_hashes.is_some() {
            return Ok(true);
        }

        if let Some(main_node_client) = &self.main_node_client {
            tracing::info!("Fetching protocol version {protocol_version:?} from the main node");

            let protocol_version_info = main_node_client
                .fetch_protocol_version(protocol_version)
                .await
                .context("failed to fetch protocol version from the main node")?
                .context("protocol version is missing on the main node")?;
            self.pool
                .connection_tagged("sync_layer")
                .await?
                .protocol_versions_dal()
                .save_protocol_version(
                    ProtocolSemanticVersion {
                        minor: protocol_version_info
                            .minor_version
                            .try_into()
                            .context("cannot convert protocol version")?,
                        patch: VersionPatch(0),
                    },
                    protocol_version_info.timestamp,
                    Default::default(), // verification keys are unused for EN
                    BaseSystemContractsHashes {
                        bootloader: protocol_version_info.bootloader_code_hash,
                        default_aa: protocol_version_info.default_account_code_hash,
                        evm_emulator: protocol_version_info.evm_emulator_code_hash,
                    },
                    protocol_version_info.l2_system_upgrade_tx_hash,
                )
                .await?;
            return Ok(true);
        } else {
            while Instant::now() < deadline {
                let base_system_contract_hashes = self
                    .pool
                    .connection_tagged("sync_layer")
                    .await?
                    .protocol_versions_dal()
                    .get_base_system_contract_hashes_by_version_id(protocol_version)
                    .await?;
                if base_system_contract_hashes.is_some() {
                    return Ok(true);
                } else {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }

        Ok(false)
    }

    pub fn set_open_batch(&mut self, open_batch: Option<IOOpenBatch>) {
        self.open_batch = open_batch;
    }
}

#[async_trait]
impl IoSealCriteria for ExternalIO {
    async fn should_seal_l1_batch_unconditionally(
        &mut self,
        _manager: &UpdatesManager,
    ) -> anyhow::Result<bool> {
        if !matches!(self.actions.peek_action(), Some(SyncAction::SealBatch)) {
            return Ok(false);
        }
        self.actions.pop_action();
        Ok(true)
    }

    fn should_seal_l2_block(&mut self, _manager: &UpdatesManager) -> bool {
        if !matches!(self.actions.peek_action(), Some(SyncAction::SealL2Block)) {
            return false;
        }
        self.actions.pop_action();
        true
    }
}

#[async_trait]
impl StateKeeperIO for ExternalIO {
    fn chain_id(&self) -> L2ChainId {
        self.chain_id
    }

    async fn initialize(&mut self) -> anyhow::Result<(IoCursor, Option<PendingBatchData>)> {
        let mut storage = self.pool.connection_tagged("sync_layer").await?;
        let cursor = IoCursor::new(&mut storage).await?;
        self.l1_batch_params_provider
            .initialize(&mut storage)
            .await
            .context("failed initializing L1 batch params provider")?;

        L2BlockSealProcess::clear_pending_l2_block(&mut storage, cursor.next_l2_block - 1).await?;

        let Some(restored_l1_batch_env) = self
            .l1_batch_params_provider
            .load_l1_batch_env(
                &mut storage,
                cursor.l1_batch,
                super::VALIDATION_COMPUTATIONAL_GAS_LIMIT,
                self.chain_id,
            )
            .await?
        else {
            storage
                .blocks_dal()
                .delete_unsealed_l1_batch(cursor.l1_batch - 1)
                .await?;
            return Ok((cursor, None));
        };
        let pending_batch_data = load_pending_batch(&mut storage, restored_l1_batch_env)
            .await
            .with_context(|| {
                format!(
                    "failed loading data for re-execution for pending L1 batch #{}",
                    cursor.l1_batch
                )
            })?;

        storage
            .blocks_dal()
            .ensure_unsealed_l1_batch_exists(
                pending_batch_data
                    .l1_batch_env
                    .clone()
                    .into_unsealed_header(
                        Some(pending_batch_data.system_env.version),
                        pending_batch_data.pubdata_limit,
                    ),
            )
            .await?;
        self.open_batch = Some(IOOpenBatch {
            number: pending_batch_data.l1_batch_env.number,
            protocol_version: pending_batch_data.system_env.version,
        });

        Ok((cursor, Some(pending_batch_data)))
    }

    async fn wait_for_new_batch_params(
        &mut self,
        cursor: &IoCursor,
        max_wait: Duration,
    ) -> anyhow::Result<Option<L1BatchParams>> {
        tracing::debug!("Waiting for the new batch params");
        let deadline = Instant::now() + max_wait;
        let Some(action) = self.actions.recv_action(max_wait).await else {
            return Ok(None);
        };
        match action {
            SyncAction::OpenBatch {
                params,
                number,
                first_l2_block_number,
            } => {
                anyhow::ensure!(
                    number == cursor.l1_batch,
                    "Batch number mismatch: expected {}, got {number}",
                    cursor.l1_batch
                );
                anyhow::ensure!(
                    first_l2_block_number == cursor.next_l2_block,
                    "L2 block number mismatch: expected {}, got {first_l2_block_number}",
                    cursor.next_l2_block
                );

                if !self
                    .is_protocol_version_saved(params.protocol_version, deadline)
                    .await?
                {
                    return Ok(None);
                }
                let mut conn = self.pool.connection_tagged("sync_layer").await?;
                conn.blocks_dal()
                    .insert_l1_batch(UnsealedL1BatchHeader {
                        number: cursor.l1_batch,
                        timestamp: params.first_l2_block.timestamp(),
                        protocol_version: Some(params.protocol_version),
                        fee_address: params.operator_address,
                        fee_input: params.fee_input,
                        pubdata_limit: params.pubdata_limit,
                    })
                    .await?;
                self.open_batch = Some(IOOpenBatch {
                    number: cursor.l1_batch,
                    protocol_version: params.protocol_version,
                });
                return Ok(Some(params));
            }
            other => {
                anyhow::bail!("unexpected action in the action queue: {other:?}");
            }
        }
    }

    async fn wait_for_new_l2_block_params(
        &mut self,
        cursor: &IoCursor,
        max_wait: Duration,
    ) -> anyhow::Result<Option<L2BlockParams>> {
        // Wait for the next L2 block to appear in the queue.
        let Some(action) = self.actions.recv_action(max_wait).await else {
            return Ok(None);
        };
        match action {
            SyncAction::L2Block { params, number } => {
                anyhow::ensure!(
                    number == cursor.next_l2_block,
                    "L2 block number mismatch: expected {}, got {number}",
                    cursor.next_l2_block
                );
                return Ok(Some(params));
            }
            other => {
                anyhow::bail!(
                    "Unexpected action in the queue while waiting for the next L2 block: {other:?}"
                );
            }
        }
    }

    fn update_next_l2_block_timestamp(&mut self, _block_timestamp: &mut u64) {}

    async fn wait_for_next_tx(
        &mut self,
        max_wait: Duration,
        _l2_block_timestamp: u64,
    ) -> anyhow::Result<Option<Transaction>> {
        tracing::debug!(
            "Waiting for the new tx, next action is {:?}",
            self.actions.peek_action()
        );
        let Some(action) = self.actions.peek_action_async(max_wait).await else {
            return Ok(None);
        };
        match action {
            SyncAction::Tx(tx) => {
                self.actions.pop_action().unwrap();
                return Ok(Some(Transaction::from(*tx)));
            }
            SyncAction::SealL2Block | SyncAction::SealBatch => {
                // No more transactions in the current L2 block; the state keeper should seal it.
                return Ok(None);
            }
            other => {
                anyhow::bail!(
                    "Unexpected action in the queue while waiting for the next transaction: {other:?}"
                );
            }
        }
    }

    async fn rollback(&mut self, tx: Transaction) -> anyhow::Result<()> {
        // We are replaying the already sealed batches so no rollbacks are expected to occur.
        anyhow::bail!("Rollback requested. Transaction hash: {:?}", tx.hash());
    }

    async fn rollback_l2_block(
        &mut self,
        _txs: Vec<Transaction>,
        first_block_in_batch: bool,
    ) -> anyhow::Result<()> {
        if first_block_in_batch {
            let current_batch_number = self.open_batch.context("`open_batch` is missing")?.number;
            let mut conn = self.pool.connection_tagged("sync_layer").await?;
            conn.blocks_dal()
                .delete_unsealed_l1_batch(current_batch_number - 1)
                .await?;
            self.open_batch = None;
        }

        self.actions.validate_ready_for_next_block();
        Ok(())
    }

    async fn advance_mempool(
        &mut self,
        _txs: Box<&mut (dyn Iterator<Item = &Transaction> + Send)>,
    ) {
        // Do nothing
    }

    async fn reject(&mut self, tx: &Transaction, reason: UnexecutableReason) -> anyhow::Result<()> {
        // We are replaying the already executed transactions so no rejections are expected to occur.
        anyhow::bail!(
            "Requested rejection of transaction {:?} because of the following error: {reason}. \
             This is not supported on external node",
            tx.hash()
        );
    }

    async fn load_base_system_contracts(
        &self,
        protocol_version: ProtocolVersionId,
        _cursor: &IoCursor,
    ) -> anyhow::Result<BaseSystemContracts> {
        get_base_system_contracts_by_version_id(
            &mut self.pool.connection_tagged("sync_layer").await?,
            protocol_version,
        )
        .await
        .context("failed loading base system contracts")?
        .with_context(|| {
            format!("no base system contracts persisted for protocol version {protocol_version:?}")
        })
    }

    async fn load_batch_version_id(
        &self,
        number: L1BatchNumber,
    ) -> anyhow::Result<ProtocolVersionId> {
        let mut storage = self.pool.connection_tagged("sync_layer").await?;
        self.l1_batch_params_provider
            .load_l1_batch_protocol_version(&mut storage, number)
            .await
            .with_context(|| format!("failed loading protocol version for L1 batch #{number}"))?
            .with_context(|| format!("L1 batch #{number} misses protocol version"))
    }

    async fn load_upgrade_tx(
        &self,
        _version_id: ProtocolVersionId,
    ) -> anyhow::Result<Option<ProtocolUpgradeTx>> {
        // External node will fetch upgrade tx from the main node
        Ok(None)
    }

    async fn load_batch_state_hash(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<H256> {
        tracing::info!("Getting L1 batch hash for L1 batch #{l1_batch_number}");
        let mut storage = self.pool.connection_tagged("sync_layer").await?;
        let wait_latency = KEEPER_METRICS.wait_for_prev_hash_time.start();
        let (hash, _) = self
            .l1_batch_params_provider
            .wait_for_l1_batch_params(&mut storage, l1_batch_number)
            .await
            .with_context(|| format!("error waiting for params for L1 batch #{l1_batch_number}"))?;
        wait_latency.observe();
        Ok(hash)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use zksync_dal::{ConnectionPool, CoreDal};
    use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
    use zksync_state_keeper::{io::L1BatchParams, L2BlockParams, StateKeeperIO};
    use zksync_types::{
        api, fee_model::BatchFeeInput, L1BatchNumber, L2BlockNumber, L2ChainId, ProtocolVersionId,
        H256,
    };

    use crate::{sync_action::SyncAction, testonly::MockMainNodeClient, ActionQueue, ExternalIO};

    #[tokio::test]
    async fn insert_batch_with_protocol_version() {
        // Whenever ExternalIO inserts an unsealed batch into DB it should populate it with protocol
        // version and make sure that it is present in the DB (i.e. fetch it from main node if not).
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        insert_genesis_batch(&mut conn, &GenesisParams::mock())
            .await
            .unwrap();
        let (actions_sender, action_queue) = ActionQueue::new();
        let mut client = MockMainNodeClient::default();
        let next_protocol_version = api::ProtocolVersionInfo {
            minor_version: ProtocolVersionId::next() as u16,
            timestamp: 1,
            bootloader_code_hash: H256::repeat_byte(1),
            default_account_code_hash: H256::repeat_byte(1),
            evm_emulator_code_hash: Some(H256::repeat_byte(1)),
            l2_system_upgrade_tx_hash: None,
        };
        client.insert_protocol_version(next_protocol_version.clone());
        let mut external_io = ExternalIO::new(
            pool.clone(),
            action_queue,
            Some(Box::new(client)),
            L2ChainId::default(),
        )
        .unwrap();

        let (cursor, _) = external_io.initialize().await.unwrap();
        let params = L1BatchParams {
            protocol_version: ProtocolVersionId::next(),
            validation_computational_gas_limit: u32::MAX,
            operator_address: Default::default(),
            fee_input: BatchFeeInput::pubdata_independent(2, 3, 4),
            first_l2_block: L2BlockParams::new(1000),
            pubdata_params: Default::default(),
            pubdata_limit: Some(100_000),
        };
        actions_sender
            .push_action_unchecked(SyncAction::OpenBatch {
                params: params.clone(),
                number: L1BatchNumber(1),
                first_l2_block_number: L2BlockNumber(1),
            })
            .await
            .unwrap();
        let fetched_params = external_io
            .wait_for_new_batch_params(&cursor, Duration::from_secs(10))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched_params, params);

        // Verify that the next protocol version is in DB
        let fetched_protocol_version = conn
            .protocol_versions_dal()
            .get_protocol_version_with_latest_patch(ProtocolVersionId::next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            fetched_protocol_version.version.minor as u16,
            next_protocol_version.minor_version
        );

        // Verify that the unsealed batch has protocol version
        let unsealed_batch = conn
            .blocks_dal()
            .get_unsealed_l1_batch()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            unsealed_batch.protocol_version,
            Some(fetched_protocol_version.version.minor)
        );
    }
}

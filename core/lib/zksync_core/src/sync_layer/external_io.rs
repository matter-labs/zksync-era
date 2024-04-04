use std::{collections::HashMap, time::Duration};

use anyhow::Context as _;
use async_trait::async_trait;
use vm_utils::storage::L1BatchParamsProvider;
use zksync_contracts::{BaseSystemContracts, BaseSystemContractsHashes, SystemContractCode};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::{
    protocol_upgrade::ProtocolUpgradeTx, L1BatchNumber, L2ChainId, MiniblockNumber,
    ProtocolVersionId, Transaction, H256,
};
use zksync_utils::bytes_to_be_words;

use super::{
    client::MainNodeClient,
    sync_action::{ActionQueue, SyncAction},
};
use crate::state_keeper::{
    io::{
        common::{load_pending_batch, poll_iters, IoCursor},
        fee_address_migration, L1BatchParams, MiniblockParams, PendingBatchData, StateKeeperIO,
    },
    metrics::KEEPER_METRICS,
    seal_criteria::IoSealCriteria,
    updates::UpdatesManager,
};

/// The interval between the action queue polling attempts for the new actions.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// ExternalIO is the IO abstraction for the state keeper that is used in the external node.
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
    main_node_client: Box<dyn MainNodeClient>,
    chain_id: L2ChainId,
}

impl ExternalIO {
    pub async fn new(
        pool: ConnectionPool<Core>,
        actions: ActionQueue,
        main_node_client: Box<dyn MainNodeClient>,
        chain_id: L2ChainId,
    ) -> anyhow::Result<Self> {
        let mut storage = pool.connection_tagged("sync_layer").await?;
        let l1_batch_params_provider = L1BatchParamsProvider::new(&mut storage)
            .await
            .context("failed initializing L1 batch params provider")?;
        // We must run the migration for pending miniblocks synchronously, since we use `fee_account_address`
        // from a pending miniblock in `load_pending_batch()` implementation.
        fee_address_migration::migrate_pending_miniblocks(&mut storage).await?;
        drop(storage);

        Ok(Self {
            pool,
            l1_batch_params_provider,
            actions,
            main_node_client,
            chain_id,
        })
    }

    async fn get_base_system_contract(
        &self,
        hash: H256,
        current_miniblock_number: MiniblockNumber,
    ) -> anyhow::Result<SystemContractCode> {
        let bytecode = self
            .pool
            .connection_tagged("sync_layer")
            .await?
            .factory_deps_dal()
            .get_factory_dep(hash)
            .await?;

        Ok(match bytecode {
            Some(bytecode) => SystemContractCode {
                code: bytes_to_be_words(bytecode),
                hash,
            },
            None => {
                tracing::info!(
                    "Fetching base system contract bytecode with hash {hash:?} from the main node"
                );

                let contract_bytecode = self
                    .main_node_client
                    .fetch_system_contract_by_hash(hash)
                    .await
                    .context("failed to fetch base system contract bytecode from the main node")?
                    .context("base system contract is missing on the main node")?;
                self.pool
                    .connection_tagged("sync_layer")
                    .await?
                    .factory_deps_dal()
                    .insert_factory_deps(
                        current_miniblock_number,
                        &HashMap::from([(hash, contract_bytecode.clone())]),
                    )
                    .await
                    .context("failed persisting system contract")?;
                SystemContractCode {
                    code: bytes_to_be_words(contract_bytecode),
                    hash,
                }
            }
        })
    }
}

impl IoSealCriteria for ExternalIO {
    fn should_seal_l1_batch_unconditionally(&mut self, _manager: &UpdatesManager) -> bool {
        if !matches!(self.actions.peek_action(), Some(SyncAction::SealBatch)) {
            return false;
        }
        self.actions.pop_action();
        true
    }

    fn should_seal_miniblock(&mut self, _manager: &UpdatesManager) -> bool {
        if !matches!(self.actions.peek_action(), Some(SyncAction::SealMiniblock)) {
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
        tracing::info!(
            "Initialized the ExternalIO: current L1 batch number {}, current miniblock number {}",
            cursor.l1_batch,
            cursor.next_miniblock,
        );

        let pending_miniblock_header = self
            .l1_batch_params_provider
            .load_first_miniblock_in_batch(&mut storage, cursor.l1_batch)
            .await
            .with_context(|| {
                format!(
                    "failed loading first miniblock for L1 batch #{}",
                    cursor.l1_batch
                )
            })?;
        let Some(mut pending_miniblock_header) = pending_miniblock_header else {
            return Ok((cursor, None));
        };

        if !pending_miniblock_header.has_protocol_version() {
            let pending_miniblock_number = pending_miniblock_header.number();
            // Fetch protocol version ID for pending miniblocks to know which VM to use to re-execute them.
            let sync_block = self
                .main_node_client
                .fetch_l2_block(pending_miniblock_number, false)
                .await
                .context("failed to fetch block from the main node")?
                .with_context(|| {
                    format!("pending miniblock #{pending_miniblock_number} is missing on main node")
                })?;
            // Loading base system contracts will insert protocol version in the database if it's not present there.
            let protocol_version = sync_block.protocol_version;
            drop(storage);
            self.load_base_system_contracts(protocol_version, &cursor)
                .await
                .with_context(|| {
                    format!("cannot load base system contracts for {protocol_version:?}")
                })?;
            storage = self.pool.connection_tagged("sync_layer").await?;
            storage
                .blocks_dal()
                .set_protocol_version_for_pending_miniblocks(protocol_version)
                .await
                .context("failed setting protocol version for pending miniblocks")?;
            pending_miniblock_header.set_protocol_version(protocol_version);
        }

        let (system_env, l1_batch_env) = self
            .l1_batch_params_provider
            .load_l1_batch_params(
                &mut storage,
                &pending_miniblock_header,
                super::VALIDATION_COMPUTATIONAL_GAS_LIMIT,
                self.chain_id,
            )
            .await
            .with_context(|| {
                format!(
                    "failed loading parameters for pending L1 batch #{}",
                    cursor.l1_batch
                )
            })?;
        let data = load_pending_batch(&mut storage, system_env, l1_batch_env)
            .await
            .with_context(|| {
                format!(
                    "failed loading data for re-execution for pending L1 batch #{}",
                    cursor.l1_batch
                )
            })?;
        Ok((cursor, Some(data)))
    }

    async fn wait_for_new_batch_params(
        &mut self,
        cursor: &IoCursor,
        max_wait: Duration,
    ) -> anyhow::Result<Option<L1BatchParams>> {
        tracing::debug!("Waiting for the new batch params");
        for _ in 0..poll_iters(POLL_INTERVAL, max_wait) {
            match self.actions.pop_action() {
                Some(SyncAction::OpenBatch {
                    params,
                    number,
                    first_miniblock_number,
                }) => {
                    anyhow::ensure!(
                        number == cursor.l1_batch,
                        "Batch number mismatch: expected {}, got {number}",
                        cursor.l1_batch
                    );
                    anyhow::ensure!(
                        first_miniblock_number == cursor.next_miniblock,
                        "Miniblock number mismatch: expected {}, got {first_miniblock_number}",
                        cursor.next_miniblock
                    );
                    return Ok(Some(params));
                }
                Some(other) => {
                    anyhow::bail!("unexpected action in the action queue: {other:?}");
                }
                None => {
                    tokio::time::sleep(POLL_INTERVAL).await;
                }
            }
        }
        Ok(None)
    }

    async fn wait_for_new_miniblock_params(
        &mut self,
        cursor: &IoCursor,
        max_wait: Duration,
    ) -> anyhow::Result<Option<MiniblockParams>> {
        // Wait for the next miniblock to appear in the queue.
        let actions = &mut self.actions;
        for _ in 0..poll_iters(POLL_INTERVAL, max_wait) {
            match actions.pop_action() {
                Some(SyncAction::Miniblock { params, number }) => {
                    anyhow::ensure!(
                        number == cursor.next_miniblock,
                        "Miniblock number mismatch: expected {}, got {number}",
                        cursor.next_miniblock
                    );
                    return Ok(Some(params));
                }
                Some(other) => {
                    anyhow::bail!(
                        "Unexpected action in the queue while waiting for the next miniblock: {other:?}"
                    );
                }
                None => {
                    tokio::time::sleep(POLL_INTERVAL).await;
                }
            }
        }
        Ok(None)
    }

    async fn wait_for_next_tx(
        &mut self,
        max_wait: Duration,
    ) -> anyhow::Result<Option<Transaction>> {
        let actions = &mut self.actions;
        tracing::debug!(
            "Waiting for the new tx, next action is {:?}",
            actions.peek_action()
        );
        for _ in 0..poll_iters(POLL_INTERVAL, max_wait) {
            match actions.peek_action() {
                Some(SyncAction::Tx(_)) => {
                    let SyncAction::Tx(tx) = actions.pop_action().unwrap() else {
                        unreachable!()
                    };
                    return Ok(Some(Transaction::from(*tx)));
                }
                Some(SyncAction::SealMiniblock | SyncAction::SealBatch) => {
                    // No more transactions in the current miniblock; the state keeper should seal it.
                    return Ok(None);
                }
                Some(other) => {
                    anyhow::bail!(
                        "Unexpected action in the queue while waiting for the next transaction: {other:?}"
                    );
                }
                _ => {
                    tokio::time::sleep(POLL_INTERVAL).await;
                }
            }
        }
        Ok(None)
    }

    async fn rollback(&mut self, tx: Transaction) -> anyhow::Result<()> {
        // We are replaying the already sealed batches so no rollbacks are expected to occur.
        anyhow::bail!("Rollback requested. Transaction hash: {:?}", tx.hash());
    }

    async fn reject(&mut self, tx: &Transaction, error: &str) -> anyhow::Result<()> {
        // We are replaying the already executed transactions so no rejections are expected to occur.
        anyhow::bail!(
            "Requested rejection of transaction {:?} because of the following error: {error}. \
             This is not supported on external node",
            tx.hash()
        );
    }
    async fn load_base_system_contracts(
        &mut self,
        protocol_version: ProtocolVersionId,
        cursor: &IoCursor,
    ) -> anyhow::Result<BaseSystemContracts> {
        let base_system_contracts = self
            .pool
            .connection_tagged("sync_layer")
            .await?
            .protocol_versions_dal()
            .load_base_system_contracts_by_version_id(protocol_version as u16)
            .await
            .context("failed loading base system contracts")?;

        if let Some(contracts) = base_system_contracts {
            return Ok(contracts);
        }
        tracing::info!("Fetching protocol version {protocol_version:?} from the main node");

        let protocol_version = self
            .main_node_client
            .fetch_protocol_version(protocol_version)
            .await
            .context("failed to fetch protocol version from the main node")?
            .context("protocol version is missing on the main node")?;
        self.pool
            .connection_tagged("sync_layer")
            .await?
            .protocol_versions_dal()
            .save_protocol_version(
                protocol_version
                    .version_id
                    .try_into()
                    .context("cannot convert protocol version")?,
                protocol_version.timestamp,
                protocol_version.verification_keys_hashes,
                protocol_version.base_system_contracts,
                protocol_version.l2_system_upgrade_tx_hash,
            )
            .await;

        let BaseSystemContractsHashes {
            bootloader,
            default_aa,
        } = protocol_version.base_system_contracts;
        let bootloader = self
            .get_base_system_contract(bootloader, cursor.next_miniblock)
            .await
            .with_context(|| format!("cannot fetch bootloader code for {protocol_version:?}"))?;
        let default_aa = self
            .get_base_system_contract(default_aa, cursor.next_miniblock)
            .await
            .with_context(|| format!("cannot fetch default AA code for {protocol_version:?}"))?;
        Ok(BaseSystemContracts {
            bootloader,
            default_aa,
        })
    }

    async fn load_batch_version_id(
        &mut self,
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
        &mut self,
        _version_id: ProtocolVersionId,
    ) -> anyhow::Result<Option<ProtocolUpgradeTx>> {
        // External node will fetch upgrade tx from the main node
        Ok(None)
    }

    async fn load_batch_state_hash(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<H256> {
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

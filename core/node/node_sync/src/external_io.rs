use std::{collections::HashMap, time::Duration};

use anyhow::Context as _;
use async_trait::async_trait;
use zksync_contracts::{BaseSystemContracts, BaseSystemContractsHashes, SystemContractCode};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_state_keeper::{
    io::{
        common::{load_pending_batch, IoCursor},
        seal_logic::l2_block_seal_subtasks::L2BlockSealProcess,
        L1BatchParams, L2BlockParams, PendingBatchData, StateKeeperIO,
    },
    metrics::KEEPER_METRICS,
    seal_criteria::{IoSealCriteria, UnexecutableReason},
    updates::UpdatesManager,
};
use zksync_types::{
    protocol_upgrade::ProtocolUpgradeTx,
    protocol_version::{ProtocolSemanticVersion, VersionPatch},
    L1BatchNumber, L2BlockNumber, L2ChainId, ProtocolVersionId, Transaction, H256,
};
use zksync_utils::bytes_to_be_words;
use zksync_vm_executor::storage::L1BatchParamsProvider;

use super::{
    client::MainNodeClient,
    sync_action::{ActionQueue, SyncAction},
};

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
    pub fn new(
        pool: ConnectionPool<Core>,
        actions: ActionQueue,
        main_node_client: Box<dyn MainNodeClient>,
        chain_id: L2ChainId,
    ) -> anyhow::Result<Self> {
        let l1_batch_params_provider = L1BatchParamsProvider::new();
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
        current_l2_block_number: L2BlockNumber,
    ) -> anyhow::Result<SystemContractCode> {
        let bytecode = self
            .pool
            .connection_tagged("sync_layer")
            .await?
            .factory_deps_dal()
            .get_sealed_factory_dep(hash)
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
                    .await?
                    .context("base system contract is missing on the main node")?;
                self.pool
                    .connection_tagged("sync_layer")
                    .await?
                    .factory_deps_dal()
                    .insert_factory_deps(
                        current_l2_block_number,
                        &HashMap::from([(hash, contract_bytecode.clone())]),
                    )
                    .await?;
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
        tracing::info!(
            "Initialized the ExternalIO: current L1 batch number {}, current L2 block number {}",
            cursor.l1_batch,
            cursor.next_l2_block,
        );

        L2BlockSealProcess::clear_pending_l2_block(&mut storage, cursor.next_l2_block - 1).await?;
        let pending_l2_block_header = self
            .l1_batch_params_provider
            .load_first_l2_block_in_batch(&mut storage, cursor.l1_batch)
            .await
            .with_context(|| {
                format!(
                    "failed loading first L2 block for L1 batch #{}",
                    cursor.l1_batch
                )
            })?;
        let Some(mut pending_l2_block_header) = pending_l2_block_header else {
            return Ok((cursor, None));
        };

        if !pending_l2_block_header.has_protocol_version() {
            let pending_l2_block_number = pending_l2_block_header.number();
            // Fetch protocol version ID for pending L2 blocks to know which VM to use to re-execute them.
            let sync_block = self
                .main_node_client
                .fetch_l2_block(pending_l2_block_number, false)
                .await
                .context("failed to fetch block from the main node")?
                .with_context(|| {
                    format!("pending L2 block #{pending_l2_block_number} is missing on main node")
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
                .set_protocol_version_for_pending_l2_blocks(protocol_version)
                .await
                .context("failed setting protocol version for pending L2 blocks")?;
            pending_l2_block_header.set_protocol_version(protocol_version);
        }

        let (system_env, l1_batch_env) = self
            .l1_batch_params_provider
            .load_l1_batch_params(
                &mut storage,
                &pending_l2_block_header,
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

    async fn wait_for_next_tx(
        &mut self,
        max_wait: Duration,
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
        let minor = protocol_version
            .minor_version()
            .context("Missing minor protocol version")?;
        let bootloader_code_hash = protocol_version
            .bootloader_code_hash()
            .context("Missing bootloader code hash")?;
        let default_account_code_hash = protocol_version
            .default_account_code_hash()
            .context("Missing default account code hash")?;
        let evm_simulator_code_hash = protocol_version
            .evm_simulator_code_hash()
            .context("Missing evm simulator code hash")?;
        let l2_system_upgrade_tx_hash = protocol_version.l2_system_upgrade_tx_hash();
        self.pool
            .connection_tagged("sync_layer")
            .await?
            .protocol_versions_dal()
            .save_protocol_version(
                ProtocolSemanticVersion {
                    minor: minor
                        .try_into()
                        .context("cannot convert protocol version")?,
                    patch: VersionPatch(0),
                },
                protocol_version.timestamp,
                Default::default(), // verification keys are unused for EN
                BaseSystemContractsHashes {
                    bootloader: bootloader_code_hash,
                    default_aa: default_account_code_hash,
                    evm_simulator: evm_simulator_code_hash,
                },
                l2_system_upgrade_tx_hash,
            )
            .await?;

        let bootloader = self
            .get_base_system_contract(bootloader_code_hash, cursor.next_l2_block)
            .await
            .with_context(|| format!("cannot fetch bootloader code for {protocol_version:?}"))?;
        let default_aa = self
            .get_base_system_contract(default_account_code_hash, cursor.next_l2_block)
            .await
            .with_context(|| format!("cannot fetch default AA code for {protocol_version:?}"))?;
        let evm_simulator = self
            .get_base_system_contract(evm_simulator_code_hash, cursor.next_l2_block)
            .await
            .with_context(|| format!("cannot fetch EVM simulator code for {protocol_version:?}"))?;
        Ok(BaseSystemContracts {
            bootloader,
            default_aa,
            evm_simulator,
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

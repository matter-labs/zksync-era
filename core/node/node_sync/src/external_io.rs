use std::{collections::HashMap, fmt::Debug, time::Duration};

use anyhow::Context as _;
use async_trait::async_trait;
use zksync_contracts::{
    hyperchain_contract, BaseSystemContracts, BaseSystemContractsHashes, SystemContractCode,
};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_eth_client::EthInterface;
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
    block::UnsealedL1BatchHeader,
    l1::L1Tx,
    protocol_upgrade::ProtocolUpgradeTx,
    protocol_version::{ProtocolSemanticVersion, VersionPatch},
    u256_to_h256,
    web3::FilterBuilder,
    Address, L1BatchNumber, L2BlockNumber, L2ChainId, PriorityOpId, ProtocolVersionId, Transaction,
    H256,
};
use zksync_vm_executor::storage::L1BatchParamsProvider;
use zksync_web3_decl::client::{DynClient, L1};

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
    main_node_client: Box<dyn MainNodeClient>,
    chain_id: L2ChainId,
    txs_verifier: Box<dyn PriorityTransactionVerifier>,
}

impl ExternalIO {
    pub fn new(
        pool: ConnectionPool<Core>,
        actions: ActionQueue,
        main_node_client: Box<dyn MainNodeClient>,
        chain_id: L2ChainId,
        txs_verifier: Box<dyn PriorityTransactionVerifier>,
    ) -> anyhow::Result<Self> {
        let l1_batch_params_provider = L1BatchParamsProvider::uninitialized();
        Ok(Self {
            pool,
            l1_batch_params_provider,
            actions,
            main_node_client,
            chain_id,
            txs_verifier,
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
                code: bytecode,
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
                    code: contract_bytecode,
                    hash,
                }
            }
        })
    }

    async fn ensure_protocol_version_is_saved(
        &self,
        protocol_version: ProtocolVersionId,
    ) -> anyhow::Result<()> {
        let base_system_contract_hashes = self
            .pool
            .connection_tagged("sync_layer")
            .await?
            .protocol_versions_dal()
            .get_base_system_contract_hashes_by_version_id(protocol_version)
            .await?;
        if base_system_contract_hashes.is_some() {
            return Ok(());
        }
        tracing::info!("Fetching protocol version {protocol_version:?} from the main node");

        let protocol_version_info = self
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
        Ok(())
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
            tracing::info!(
                l1_batch_number = %cursor.l1_batch,
                "No pending L2 blocks found; pruning unsealed batch if exists as we need at least one L2 block to initialize"
            );
            storage
                .blocks_dal()
                .delete_unsealed_l1_batch(cursor.l1_batch - 1)
                .await?;
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

        let restored_l1_batch_env = self
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
        storage
            .blocks_dal()
            .ensure_unsealed_l1_batch_exists(
                restored_l1_batch_env
                    .l1_batch_env
                    .clone()
                    .into_unsealed_header(
                        Some(restored_l1_batch_env.system_env.version),
                        restored_l1_batch_env.pubdata_limit,
                    ),
            )
            .await?;
        let data = load_pending_batch(&mut storage, restored_l1_batch_env)
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

                self.ensure_protocol_version_is_saved(params.protocol_version)
                    .await?;
                self.pool
                    .connection_tagged("sync_layer")
                    .await?
                    .blocks_dal()
                    .insert_l1_batch(UnsealedL1BatchHeader {
                        number: cursor.l1_batch,
                        timestamp: params.first_l2_block.timestamp(),
                        protocol_version: Some(params.protocol_version),
                        fee_address: params.operator_address,
                        fee_input: params.fee_input,
                        pubdata_limit: params.pubdata_limit,
                    })
                    .await?;
                Ok(Some(params))
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
                Ok(Some(params))
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
                let tx = Transaction::from(*tx);
                if let Some(l1_tx) = &tx.l1_tx() {
                    let max_attempts = 5;
                    for i in 0..=max_attempts {
                        if let Err(err) = self.txs_verifier.verify_transaction(l1_tx).await {
                            if err.is_retriable() && i < max_attempts {
                                tracing::warn!(
                                    "Failed to verify transaction {:?} with error: {err}. Retrying...",
                                    tx.hash()
                                );
                                tokio::time::sleep(Duration::from_secs(1)).await;
                                continue;
                            } else {
                                tracing::error!(
                                    "Transaction {:?} verification failed with error: {err}",
                                    tx.hash()
                                );
                                anyhow::bail!(err);
                            }
                        }
                    }
                }
                Ok(Some(tx))
            }
            SyncAction::SealL2Block | SyncAction::SealBatch => {
                // No more transactions in the current L2 block; the state keeper should seal it.
                Ok(None)
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

    async fn rollback_l2_block(&mut self, _txs: Vec<Transaction>) -> anyhow::Result<()> {
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
        cursor: &IoCursor,
    ) -> anyhow::Result<BaseSystemContracts> {
        let base_system_contracts = self
            .pool
            .connection_tagged("sync_layer")
            .await?
            .protocol_versions_dal()
            .get_base_system_contract_hashes_by_version_id(protocol_version)
            .await?
            .with_context(|| {
                format!("Cannot load base system contracts' hashes for {protocol_version:?}. They should already be present")
            })?;

        let bootloader = self
            .get_base_system_contract(base_system_contracts.bootloader, cursor.next_l2_block)
            .await
            .with_context(|| format!("cannot fetch bootloader code for {protocol_version:?}"))?;
        let default_aa = self
            .get_base_system_contract(base_system_contracts.default_aa, cursor.next_l2_block)
            .await
            .with_context(|| format!("cannot fetch default AA code for {protocol_version:?}"))?;
        let evm_emulator = if let Some(hash) = base_system_contracts.evm_emulator {
            Some(
                self.get_base_system_contract(hash, cursor.next_l2_block)
                    .await
                    .with_context(|| {
                        format!("cannot fetch EVM emulator code for {protocol_version:?}")
                    })?,
            )
        } else {
            None
        };

        Ok(BaseSystemContracts {
            bootloader,
            default_aa,
            evm_emulator,
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

#[derive(Debug, thiserror::Error)]
pub enum PriorityTransactionVerificationError {
    #[error("Transaction does not match the L1 transaction with id: {0:?}")]
    TransactionDoesnntMatchL1Tx(PriorityOpId),
    #[error("Transaction doesn't exist: {0}")]
    TransactionDoesntExist(H256),
    #[error("Client error: {0}")]
    ClientError(#[from] zksync_eth_client::EnrichedClientError),
    #[error("Error: {0}")]
    Other(#[from] anyhow::Error),
}

impl PriorityTransactionVerificationError {
    fn is_retriable(&self) -> bool {
        match self {
            PriorityTransactionVerificationError::ClientError(_)
            | PriorityTransactionVerificationError::Other(_) => true,
            // Other errors are not retriable
            _ => false,
        }
    }
}

#[async_trait]
pub trait PriorityTransactionVerifier: std::fmt::Debug + Send + Sync {
    /// Verifies the transaction and returns an error if it is not executable.
    async fn verify_transaction(
        &self,
        tx: &L1Tx,
    ) -> Result<(), PriorityTransactionVerificationError>;
}

#[derive(Debug)]
pub struct PriorityTxVerifierL1 {
    l1_client: Box<DynClient<L1>>,
    l1_diamond_proxy: Address,
    new_priority_request_signature: H256,

    pub new_priority_request_id_signature: H256,
}

impl PriorityTxVerifierL1 {
    pub fn new(l1_client: Box<DynClient<L1>>, l1_diamond_proxy: Address) -> Self {
        let new_priority_request_signature = hyperchain_contract()
            .event("NewPriorityRequest")
            .context("NewPriorityRequest event is missing in ABI")
            .unwrap()
            .signature();
        let new_priority_request_id_signature = hyperchain_contract()
            .event("NewPriorityRequestId")
            .context("NewPriorityRequestId event is missing in ABI")
            .unwrap()
            .signature();

        Self {
            l1_client,
            l1_diamond_proxy,
            new_priority_request_signature,
            new_priority_request_id_signature,
        }
    }
}

#[async_trait::async_trait]
impl PriorityTransactionVerifier for PriorityTxVerifierL1 {
    async fn verify_transaction(
        &self,
        tx: &L1Tx,
    ) -> Result<(), PriorityTransactionVerificationError> {
        let filter = FilterBuilder::default()
            .address(vec![self.l1_diamond_proxy])
            .topics(
                Some(vec![self.new_priority_request_id_signature]),
                Some(vec![u256_to_h256((*tx.serial_id()).into())]),
                None,
                None,
            )
            .from_block((tx.common_data.eth_block - 1).into())
            .to_block((tx.common_data.eth_block + 1).into())
            .build();
        let mut logs = self.l1_client.logs(&filter).await?;
        if logs.len() != 1 {
            return Err(PriorityTransactionVerificationError::Other(
                anyhow::anyhow!(
                    "Expected exactly one log for transaction {}, but found {}",
                    tx.hash(),
                    logs.len()
                ),
            ));
        }

        let log = logs.remove(0);
        let tx_hash = log.transaction_hash.ok_or_else(|| {
            PriorityTransactionVerificationError::Other(anyhow::anyhow!(
                "Log for transaction {} does not have a transaction hash",
                tx.hash()
            ))
        })?;

        let data = self
            .l1_client
            .tx_receipt(tx_hash)
            .await?
            .context("Transaction receipt for the priority transaction is missing on L1")?;

        let priority_requests = data
            .logs
            .iter()
            .filter(|log| log.topics[0] == self.new_priority_request_signature)
            .map(|log| {
                L1Tx::try_from(Into::<zksync_types::web3::Log>::into(log.clone()))
                    .map_err(|err| PriorityTransactionVerificationError::Other(err.into()))
            })
            .find(|new_l1_tx| {
                let Ok(res) = new_l1_tx else { return false };
                res.serial_id() == tx.serial_id()
            })
            .transpose()?;

        let Some(found_l1_tx) = priority_requests else {
            return Err(PriorityTransactionVerificationError::TransactionDoesntExist(tx.hash()));
        };

        if found_l1_tx != *tx {
            tracing::warn!(
                "Transaction {:?} does not match the L1
                 Found L1 transaction: {:?}",
                tx,
                found_l1_tx
            );
            Err(
                PriorityTransactionVerificationError::TransactionDoesnntMatchL1Tx(
                    found_l1_tx.serial_id(),
                ),
            )
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use zksync_dal::{ConnectionPool, CoreDal};
    use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
    use zksync_state_keeper::{io::L1BatchParams, L2BlockParams, StateKeeperIO};
    use zksync_types::{
        api, fee_model::BatchFeeInput, l1::L1Tx, L1BatchNumber, L2BlockNumber, L2ChainId,
        ProtocolVersionId, H256,
    };

    use crate::{
        external_io::{PriorityTransactionVerificationError, PriorityTransactionVerifier},
        sync_action::SyncAction,
        testonly::MockMainNodeClient,
        ActionQueue, ExternalIO,
    };

    #[derive(Debug)]
    struct MockPriorityTransactionVerifier;

    #[async_trait::async_trait]
    impl PriorityTransactionVerifier for MockPriorityTransactionVerifier {
        async fn verify_transaction(
            &self,
            _tx: &L1Tx,
        ) -> Result<(), PriorityTransactionVerificationError> {
            Ok(())
        }
    }

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
            Box::new(client),
            L2ChainId::default(),
            Box::new(MockPriorityTransactionVerifier),
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

use std::{collections::HashMap, convert::TryInto, iter::FromIterator, time::Duration};

use async_trait::async_trait;
use futures::future;
use multivm::interface::{FinishedL1Batch, L1BatchEnv, SystemEnv};
use zksync_contracts::{BaseSystemContracts, SystemContractCode};
use zksync_dal::ConnectionPool;
use zksync_types::{
    ethabi::Address, fee_model::BatchFeeInput, protocol_version::ProtocolUpgradeTx,
    witness_block_state::WitnessBlockState, L1BatchNumber, L2ChainId, MiniblockNumber,
    ProtocolVersionId, Transaction, H256, U256,
};
use zksync_utils::{be_words_to_bytes, bytes_to_be_words};

use super::{
    client::MainNodeClient,
    sync_action::{ActionQueue, SyncAction},
    SyncState,
};
use crate::{
    metrics::{BlockStage, APP_METRICS},
    state_keeper::{
        extractors,
        io::{
            common::{l1_batch_params, load_pending_batch, poll_iters},
            MiniblockParams, MiniblockSealerHandle, PendingBatchData, StateKeeperIO,
        },
        metrics::KEEPER_METRICS,
        seal_criteria::IoSealCriteria,
        updates::UpdatesManager,
    },
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
    miniblock_sealer_handle: MiniblockSealerHandle,
    pool: ConnectionPool,

    current_l1_batch_number: L1BatchNumber,
    current_miniblock_number: MiniblockNumber,
    actions: ActionQueue,
    sync_state: SyncState,
    main_node_client: Box<dyn MainNodeClient>,

    /// Required to extract newly added tokens.
    l2_erc20_bridge_addr: Address,
    // TODO it's required for system env, probably we have to get rid of getting system env
    validation_computational_gas_limit: u32,
    chain_id: L2ChainId,
}

impl ExternalIO {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        miniblock_sealer_handle: MiniblockSealerHandle,
        pool: ConnectionPool,
        actions: ActionQueue,
        sync_state: SyncState,
        main_node_client: Box<dyn MainNodeClient>,
        l2_erc20_bridge_addr: Address,
        validation_computational_gas_limit: u32,
        chain_id: L2ChainId,
    ) -> Self {
        let mut storage = pool.access_storage_tagged("sync_layer").await.unwrap();
        // TODO (PLA-703): Support no L1 batches / miniblocks in the storage
        let last_sealed_l1_batch_number = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .unwrap()
            .expect("No L1 batches sealed");
        let last_miniblock_number = storage
            .blocks_dal()
            .get_sealed_miniblock_number()
            .await
            .unwrap()
            .expect("empty storage not supported"); // FIXME (PLA-703): handle empty storage
        drop(storage);

        tracing::info!(
            "Initialized the ExternalIO: current L1 batch number {}, current miniblock number {}",
            last_sealed_l1_batch_number + 1,
            last_miniblock_number + 1,
        );

        sync_state.set_local_block(last_miniblock_number);

        Self {
            miniblock_sealer_handle,
            pool,
            current_l1_batch_number: last_sealed_l1_batch_number + 1,
            current_miniblock_number: last_miniblock_number + 1,
            actions,
            sync_state,
            main_node_client,
            l2_erc20_bridge_addr,
            validation_computational_gas_limit,
            chain_id,
        }
    }

    async fn load_previous_l1_batch_hash(&self) -> U256 {
        let mut storage = self.pool.access_storage_tagged("sync_layer").await.unwrap();
        let wait_latency = KEEPER_METRICS.wait_for_prev_hash_time.start();
        let (hash, _) =
            extractors::wait_for_prev_l1_batch_params(&mut storage, self.current_l1_batch_number)
                .await;
        wait_latency.observe();
        hash
    }

    async fn load_previous_miniblock_hash(&self) -> H256 {
        let prev_miniblock_number = self.current_miniblock_number - 1;
        let mut storage = self.pool.access_storage_tagged("sync_layer").await.unwrap();
        let header = storage
            .blocks_dal()
            .get_miniblock_header(prev_miniblock_number)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("Miniblock #{prev_miniblock_number} is missing"));
        header.hash
    }

    async fn load_base_system_contracts_by_version_id(
        &self,
        id: ProtocolVersionId,
    ) -> BaseSystemContracts {
        let base_system_contracts = self
            .pool
            .access_storage_tagged("sync_layer")
            .await
            .unwrap()
            .protocol_versions_dal()
            .load_base_system_contracts_by_version_id(id as u16)
            .await;

        match base_system_contracts {
            Some(version) => version,
            None => {
                let protocol_version = self
                    .main_node_client
                    .fetch_protocol_version(id)
                    .await
                    .expect("Failed to fetch protocol version from the main node");
                self.pool
                    .access_storage_tagged("sync_layer")
                    .await
                    .unwrap()
                    .protocol_versions_dal()
                    .save_protocol_version(
                        protocol_version.version_id.try_into().unwrap(),
                        protocol_version.timestamp,
                        protocol_version.verification_keys_hashes,
                        protocol_version.base_system_contracts,
                        // Verifier is not used in the external node, so we can pass an empty
                        Default::default(),
                        protocol_version.l2_system_upgrade_tx_hash,
                    )
                    .await;

                let bootloader = self
                    .get_base_system_contract(protocol_version.base_system_contracts.bootloader)
                    .await;
                let default_aa = self
                    .get_base_system_contract(protocol_version.base_system_contracts.default_aa)
                    .await;
                BaseSystemContracts {
                    bootloader,
                    default_aa,
                }
            }
        }
    }

    async fn get_base_system_contract(&self, hash: H256) -> SystemContractCode {
        let bytecode = self
            .pool
            .access_storage_tagged("sync_layer")
            .await
            .unwrap()
            .storage_dal()
            .get_factory_dep(hash)
            .await;

        match bytecode {
            Some(bytecode) => SystemContractCode {
                code: bytes_to_be_words(bytecode),
                hash,
            },
            None => {
                tracing::info!("Fetching base system contract bytecode from the main node");
                let contract = self
                    .main_node_client
                    .fetch_system_contract_by_hash(hash)
                    .await
                    .expect("Failed to fetch base system contract bytecode from the main node");
                self.pool
                    .access_storage_tagged("sync_layer")
                    .await
                    .unwrap()
                    .storage_dal()
                    .insert_factory_deps(
                        self.current_miniblock_number,
                        &HashMap::from_iter([(contract.hash, be_words_to_bytes(&contract.code))]),
                    )
                    .await;
                contract
            }
        }
    }
}

impl IoSealCriteria for ExternalIO {
    fn should_seal_l1_batch_unconditionally(&mut self, _manager: &UpdatesManager) -> bool {
        matches!(
            self.actions.peek_action(),
            Some(SyncAction::SealBatch { .. })
        )
    }

    fn should_seal_miniblock(&mut self, _manager: &UpdatesManager) -> bool {
        matches!(self.actions.peek_action(), Some(SyncAction::SealMiniblock))
    }
}

#[async_trait]
impl StateKeeperIO for ExternalIO {
    fn current_l1_batch_number(&self) -> L1BatchNumber {
        self.current_l1_batch_number
    }

    fn current_miniblock_number(&self) -> MiniblockNumber {
        self.current_miniblock_number
    }

    async fn load_pending_batch(&mut self) -> Option<PendingBatchData> {
        let mut storage = self.pool.access_storage_tagged("sync_layer").await.unwrap();

        // TODO (BFT-99): Do not assume that fee account is the same as in previous batch.
        let fee_account = storage
            .blocks_dal()
            .get_l1_batch_header(self.current_l1_batch_number - 1)
            .await
            .unwrap()
            .unwrap_or_else(|| {
                panic!(
                    "No block header for batch {}",
                    self.current_l1_batch_number - 1
                )
            })
            .fee_account_address;
        let pending_miniblock_number = {
            let (_, last_miniblock_number_included_in_l1_batch) = storage
                .blocks_dal()
                .get_miniblock_range_of_l1_batch(self.current_l1_batch_number - 1)
                .await
                .unwrap()
                .unwrap();
            last_miniblock_number_included_in_l1_batch + 1
        };
        let pending_miniblock_header = storage
            .blocks_dal()
            .get_miniblock_header(pending_miniblock_number)
            .await
            .unwrap()?;

        if pending_miniblock_header.protocol_version.is_none() {
            // Fetch protocol version ID for pending miniblocks to know which VM to use to re-execute them.
            let sync_block = self
                .main_node_client
                .fetch_l2_block(pending_miniblock_header.number, false)
                .await
                .expect("Failed to fetch block from the main node")
                .expect("Block must exist");
            // Loading base system contracts will insert protocol version in the database if it's not present there.
            let _ = self
                .load_base_system_contracts_by_version_id(sync_block.protocol_version)
                .await;
            storage
                .blocks_dal()
                .set_protocol_version_for_pending_miniblocks(sync_block.protocol_version)
                .await
                .unwrap();
        }

        load_pending_batch(
            &mut storage,
            self.current_l1_batch_number,
            fee_account,
            self.validation_computational_gas_limit,
            self.chain_id,
        )
        .await
    }

    async fn wait_for_new_batch_params(
        &mut self,
        max_wait: Duration,
    ) -> Option<(SystemEnv, L1BatchEnv)> {
        tracing::debug!("Waiting for the new batch params");
        for _ in 0..poll_iters(POLL_INTERVAL, max_wait) {
            match self.actions.pop_action() {
                Some(SyncAction::OpenBatch {
                    number,
                    timestamp,
                    l1_gas_price,
                    l2_fair_gas_price,
                    fair_pubdata_price,
                    operator_address,
                    protocol_version,
                    first_miniblock_info: (miniblock_number, virtual_blocks),
                }) => {
                    assert_eq!(
                        number, self.current_l1_batch_number,
                        "Batch number mismatch"
                    );
                    tracing::info!("Getting previous L1 batch hash and miniblock hash");
                    let (previous_l1_batch_hash, previous_miniblock_hash) = future::join(
                        self.load_previous_l1_batch_hash(),
                        self.load_previous_miniblock_hash(),
                    )
                    .await;
                    tracing::info!(
                        "Previous L1 batch hash: {previous_l1_batch_hash}, previous miniblock hash: {previous_miniblock_hash}"
                    );

                    let base_system_contracts = self
                        .load_base_system_contracts_by_version_id(protocol_version)
                        .await;
                    return Some(l1_batch_params(
                        number,
                        operator_address,
                        timestamp,
                        previous_l1_batch_hash,
                        BatchFeeInput::for_protocol_version(
                            protocol_version,
                            l2_fair_gas_price,
                            fair_pubdata_price,
                            l1_gas_price,
                        ),
                        miniblock_number,
                        previous_miniblock_hash,
                        base_system_contracts,
                        self.validation_computational_gas_limit,
                        protocol_version,
                        virtual_blocks,
                        self.chain_id,
                    ));
                }
                Some(other) => {
                    panic!("Unexpected action in the action queue: {:?}", other);
                }
                None => {
                    tokio::time::sleep(POLL_INTERVAL).await;
                }
            }
        }
        None
    }

    async fn wait_for_new_miniblock_params(
        &mut self,
        max_wait: Duration,
        _prev_miniblock_timestamp: u64,
    ) -> Option<MiniblockParams> {
        // Wait for the next miniblock to appear in the queue.
        let actions = &mut self.actions;
        for _ in 0..poll_iters(POLL_INTERVAL, max_wait) {
            match actions.peek_action() {
                Some(SyncAction::Miniblock {
                    number,
                    timestamp,
                    virtual_blocks,
                }) => {
                    self.actions.pop_action(); // We found the miniblock, remove it from the queue.
                    assert_eq!(
                        number, self.current_miniblock_number,
                        "Miniblock number mismatch"
                    );
                    return Some(MiniblockParams {
                        timestamp,
                        virtual_blocks,
                    });
                }
                Some(SyncAction::SealBatch { virtual_blocks, .. }) => {
                    // We've reached the next batch, so this situation would be handled by the batch sealer.
                    // No need to pop the action from the queue.
                    // It also doesn't matter which timestamp we return, since there will be no more miniblocks in this
                    // batch. We return 0 to make it easy to detect if it ever appears somewhere.
                    return Some(MiniblockParams {
                        timestamp: 0,
                        virtual_blocks,
                    });
                }
                Some(other) => {
                    panic!(
                        "Unexpected action in the queue while waiting for the next miniblock {:?}",
                        other
                    );
                }
                _ => {
                    tokio::time::sleep(POLL_INTERVAL).await;
                    continue;
                }
            }
        }
        None
    }

    async fn wait_for_next_tx(&mut self, max_wait: Duration) -> Option<Transaction> {
        let actions = &mut self.actions;
        tracing::debug!(
            "Waiting for the new tx, next action is {:?}",
            actions.peek_action()
        );
        for _ in 0..poll_iters(POLL_INTERVAL, max_wait) {
            // We keep polling until we get any item from the queue.
            // Once we have the item, it'll be either a transaction, or a seal request.
            // Whatever item it is, we don't have to poll anymore and may exit, thus double option use.
            match actions.peek_action() {
                Some(SyncAction::Tx(_)) => {
                    let SyncAction::Tx(tx) = actions.pop_action().unwrap() else {
                        unreachable!()
                    };
                    return Some(*tx);
                }
                _ => {
                    tokio::time::sleep(POLL_INTERVAL).await;
                    continue;
                }
            }
        }
        None
    }

    async fn rollback(&mut self, tx: Transaction) {
        // We are replaying the already sealed batches so no rollbacks are expected to occur.
        panic!("Rollback requested. Transaction hash: {:?}", tx.hash());
    }

    async fn reject(&mut self, tx: &Transaction, error: &str) {
        // We are replaying the already executed transactions so no rejections are expected to occur.
        panic!(
            "Reject requested because of the following error: {}.\n Transaction hash is: {:?}",
            error,
            tx.hash()
        );
    }

    async fn seal_miniblock(&mut self, updates_manager: &UpdatesManager) {
        let action = self.actions.pop_action();
        let Some(SyncAction::SealMiniblock) = action else {
            panic!("State keeper requested to seal miniblock, but the next action is {action:?}");
        };

        // Now transactions are stored, and we may mark them as executed.
        let command = updates_manager.seal_miniblock_command(
            self.current_l1_batch_number,
            self.current_miniblock_number,
            self.l2_erc20_bridge_addr,
            true,
        );
        self.miniblock_sealer_handle.submit(command).await;

        self.sync_state
            .set_local_block(self.current_miniblock_number);
        tracing::info!("Miniblock {} is sealed", self.current_miniblock_number);
        self.current_miniblock_number += 1;
    }

    async fn seal_l1_batch(
        &mut self,
        // needed as part of the interface, to be removed once we transition to Merkle Paths
        _witness_block_state: Option<WitnessBlockState>,
        updates_manager: UpdatesManager,
        l1_batch_env: &L1BatchEnv,
        finished_batch: FinishedL1Batch,
    ) -> anyhow::Result<()> {
        let action = self.actions.pop_action();
        let Some(SyncAction::SealBatch { .. }) = action else {
            anyhow::bail!(
                "State keeper requested to seal the batch, but the next action is {action:?}"
            );
        };

        // We cannot start sealing an L1 batch until we've sealed all miniblocks included in it.
        self.miniblock_sealer_handle.wait_for_all_commands().await;

        let mut storage = self.pool.access_storage_tagged("sync_layer").await.unwrap();
        let mut transaction = storage.start_transaction().await.unwrap();
        updates_manager
            .seal_l1_batch(
                &mut transaction,
                self.current_miniblock_number,
                l1_batch_env,
                finished_batch,
                self.l2_erc20_bridge_addr,
            )
            .await;
        transaction.commit().await.unwrap();

        tracing::info!("Batch {} is sealed", self.current_l1_batch_number);

        // Mimic the metric emitted by the main node to reuse existing Grafana charts.
        APP_METRICS.block_number[&BlockStage::Sealed].set(self.current_l1_batch_number.0.into());

        self.sync_state
            .set_local_block(self.current_miniblock_number);
        self.current_miniblock_number += 1; // Due to fictive miniblock being sealed.
        self.current_l1_batch_number += 1;
        Ok(())
    }

    async fn load_previous_batch_version_id(&mut self) -> Option<ProtocolVersionId> {
        let mut storage = self.pool.access_storage().await.unwrap();
        storage
            .blocks_dal()
            .get_batch_protocol_version_id(self.current_l1_batch_number - 1)
            .await
            .unwrap()
    }

    async fn load_upgrade_tx(
        &mut self,
        _version_id: ProtocolVersionId,
    ) -> Option<ProtocolUpgradeTx> {
        // External node will fetch upgrade tx from the main node.
        None
    }
}

use std::{collections::HashMap, convert::TryFrom, iter::FromIterator, time::Duration};

use super::genesis::fetch_system_contract_by_hash;
use actix_rt::time::Instant;
use async_trait::async_trait;
use zksync_contracts::{BaseSystemContracts, BaseSystemContractsHashes, SystemContractCode};
use zksync_dal::ConnectionPool;
use zksync_types::{
    ethabi::Address, l1::L1Tx, l2::L2Tx, protocol_version::ProtocolUpgradeTx, L1BatchNumber,
    L1BlockNumber, MiniblockNumber, ProtocolVersionId, Transaction, H256, U256,
};
use zksync_utils::{be_words_to_bytes, bytes_to_be_words};

use crate::state_keeper::{
    extractors,
    io::{
        common::{l1_batch_params, load_pending_batch, poll_iters},
        L1BatchParams, PendingBatchData, StateKeeperIO,
    },
    seal_criteria::SealerFn,
    updates::UpdatesManager,
};

use super::{
    sync_action::{ActionQueue, SyncAction},
    SyncState,
};

/// The interval between the action queue polling attempts for the new actions.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// In the external node we don't actually decide whether we want to seal l1 batch or l2 block.
/// We must replicate the state as it's present in the main node.
/// This structure declares an "unconditional sealer" which would tell the state keeper to seal
/// blocks/batches at the same point as in the main node.
#[derive(Debug, Clone)]
pub struct ExternalNodeSealer {
    actions: ActionQueue,
}

impl ExternalNodeSealer {
    pub fn new(actions: ActionQueue) -> Self {
        Self { actions }
    }

    fn should_seal_miniblock(&self) -> bool {
        let res = matches!(self.actions.peek_action(), Some(SyncAction::SealMiniblock));
        if res {
            vlog::info!("Sealing miniblock");
        }
        res
    }

    fn should_seal_batch(&self) -> bool {
        let res = matches!(self.actions.peek_action(), Some(SyncAction::SealBatch));
        if res {
            vlog::info!("Sealing the batch");
        }
        res
    }

    pub fn into_unconditional_batch_seal_criterion(self) -> Box<SealerFn> {
        Box::new(move |_| self.should_seal_batch())
    }

    pub fn into_miniblock_seal_criterion(self) -> Box<SealerFn> {
        Box::new(move |_| self.should_seal_miniblock())
    }
}

/// ExternalIO is the IO abstraction for the state keeper that is used in the external node.
/// It receives a sequence of actions from the fetcher via the action queue and propagates it
/// into the state keeper.
///
/// It is also responsible for the persisting of data, and this slice of logic is pretty close
/// to the one in the mempool IO (which is used in the main node).
#[derive(Debug)]
pub struct ExternalIO {
    pool: ConnectionPool,

    current_l1_batch_number: L1BatchNumber,
    current_miniblock_number: MiniblockNumber,
    actions: ActionQueue,
    sync_state: SyncState,
    main_node_url: String,

    /// Required to extract newly added tokens.
    l2_erc20_bridge_addr: Address,
}

impl ExternalIO {
    pub async fn new(
        pool: ConnectionPool,
        actions: ActionQueue,
        sync_state: SyncState,
        main_node_url: String,
        l2_erc20_bridge_addr: Address,
    ) -> Self {
        let mut storage = pool.access_storage_tagged("sync_layer").await;
        let last_sealed_l1_batch_header = storage.blocks_dal().get_newest_l1_batch_header().await;
        let last_miniblock_number = storage.blocks_dal().get_sealed_miniblock_number().await;
        drop(storage);

        vlog::info!(
            "Initialized the ExternalIO: current L1 batch number {}, current miniblock number {}",
            last_sealed_l1_batch_header.number + 1,
            last_miniblock_number + 1,
        );

        sync_state.set_local_block(last_miniblock_number);

        Self {
            pool,
            current_l1_batch_number: last_sealed_l1_batch_header.number + 1,
            current_miniblock_number: last_miniblock_number + 1,
            actions,
            sync_state,
            main_node_url,
            l2_erc20_bridge_addr,
        }
    }

    async fn load_previous_l1_batch_hash(&self) -> U256 {
        let mut storage = self.pool.access_storage_tagged("sync_layer").await;

        let stage_started_at: Instant = Instant::now();
        let (hash, _) =
            extractors::wait_for_prev_l1_batch_params(&mut storage, self.current_l1_batch_number)
                .await;
        metrics::histogram!(
            "server.state_keeper.wait_for_prev_hash_time",
            stage_started_at.elapsed()
        );
        hash
    }

    async fn get_base_system_contract(&self, hash: H256) -> SystemContractCode {
        let bytecode = self
            .pool
            .access_storage_tagged("sync_layer")
            .await
            .storage_dal()
            .get_factory_dep(hash)
            .await;

        match bytecode {
            Some(bytecode) => SystemContractCode {
                code: bytes_to_be_words(bytecode),
                hash,
            },
            None => {
                let main_node_url = self.main_node_url.clone();
                vlog::info!("Fetching base system contract bytecode from the main node");
                let contract = fetch_system_contract_by_hash(&main_node_url, hash)
                    .await
                    .expect("Failed to fetch base system contract bytecode from the main node");
                self.pool
                    .access_storage_tagged("sync_layer")
                    .await
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

#[async_trait]
impl StateKeeperIO for ExternalIO {
    fn current_l1_batch_number(&self) -> L1BatchNumber {
        self.current_l1_batch_number
    }

    fn current_miniblock_number(&self) -> MiniblockNumber {
        self.current_miniblock_number
    }

    async fn load_pending_batch(&mut self) -> Option<PendingBatchData> {
        let mut storage = self.pool.access_storage_tagged("sync_layer").await;

        let fee_account = storage
            .blocks_dal()
            .get_l1_batch_header(self.current_l1_batch_number - 1)
            .await
            .unwrap_or_else(|| {
                panic!(
                    "No block header for batch {}",
                    self.current_l1_batch_number - 1
                )
            })
            .fee_account_address;
        load_pending_batch(&mut storage, self.current_l1_batch_number, fee_account).await
    }

    async fn wait_for_new_batch_params(&mut self, max_wait: Duration) -> Option<L1BatchParams> {
        vlog::debug!("Waiting for the new batch params");
        for _ in 0..poll_iters(POLL_INTERVAL, max_wait) {
            match self.actions.pop_action() {
                Some(SyncAction::OpenBatch {
                    number,
                    timestamp,
                    l1_gas_price,
                    l2_fair_gas_price,
                    base_system_contracts_hashes:
                        BaseSystemContractsHashes {
                            bootloader,
                            default_aa,
                        },
                    operator_address,
                    protocol_version,
                }) => {
                    assert_eq!(
                        number, self.current_l1_batch_number,
                        "Batch number mismatch"
                    );
                    vlog::info!("Getting previous L1 batch hash");
                    let previous_l1_batch_hash = self.load_previous_l1_batch_hash().await;
                    vlog::info!("Previous L1 batch hash: {previous_l1_batch_hash}");

                    let base_system_contracts = BaseSystemContracts {
                        bootloader: self.get_base_system_contract(bootloader).await,
                        default_aa: self.get_base_system_contract(default_aa).await,
                    };

                    return Some(l1_batch_params(
                        number,
                        operator_address,
                        timestamp,
                        previous_l1_batch_hash,
                        l1_gas_price,
                        l2_fair_gas_price,
                        base_system_contracts,
                        protocol_version.unwrap_or_default(),
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
    ) -> Option<u64> {
        // Wait for the next miniblock to appear in the queue.
        let actions = &self.actions;
        for _ in 0..poll_iters(POLL_INTERVAL, max_wait) {
            match actions.peek_action() {
                Some(SyncAction::Miniblock { number, timestamp }) => {
                    self.actions.pop_action(); // We found the miniblock, remove it from the queue.
                    assert_eq!(
                        number, self.current_miniblock_number,
                        "Miniblock number mismatch"
                    );
                    return Some(timestamp);
                }
                Some(SyncAction::SealBatch) => {
                    // We've reached the next batch, so this situation would be handled by the batch sealer.
                    // No need to pop the action from the queue.
                    // It also doesn't matter which timestamp we return, since there will be no more miniblocks in this
                    // batch. We return 0 to make it easy to detect if it ever appears somewhere.
                    return Some(0);
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
        let actions = &self.actions;
        vlog::debug!(
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
        match self.actions.pop_action() {
            Some(SyncAction::SealMiniblock) => {}
            other => panic!(
                "State keeper requested to seal miniblock, but the next action is {:?}",
                other
            ),
        };

        let mut storage = self.pool.access_storage_tagged("sync_layer").await;
        let mut transaction = storage.start_transaction().await;

        let start = Instant::now();
        // We don't store the transactions in the database until they're executed to not overcomplicate the state
        // recovery on restart. So we have to store them here.
        for tx in updates_manager.miniblock.executed_transactions.iter() {
            if let Ok(l1_tx) = L1Tx::try_from(tx.transaction.clone()) {
                let l1_block_number = L1BlockNumber(l1_tx.common_data.eth_block as u32);

                transaction
                    .transactions_dal()
                    .insert_transaction_l1(l1_tx, l1_block_number)
                    .await;
            } else if let Ok(l2_tx) = L2Tx::try_from(tx.transaction.clone()) {
                // Using `Default` for execution metrics should be OK here, since this data is not used on the EN.
                transaction
                    .transactions_dal()
                    .insert_transaction_l2(l2_tx, Default::default())
                    .await;
            } else {
                unreachable!("Transaction {:?} is neither L1 nor L2", tx.transaction);
            }
        }
        metrics::histogram!(
            "server.state_keeper.l1_batch.sealed_time_stage",
            start.elapsed(),
            "stage" => "external_node_store_transactions"
        );

        // Now transactions are stored, and we may mark them as executed.
        let command = updates_manager.seal_miniblock_command(
            self.current_l1_batch_number,
            self.current_miniblock_number,
            self.l2_erc20_bridge_addr,
        );
        command.seal(&mut transaction).await;
        transaction.commit().await;

        self.sync_state
            .set_local_block(self.current_miniblock_number);
        self.current_miniblock_number += 1;
        vlog::info!("Miniblock {} is sealed", self.current_miniblock_number);
    }

    async fn seal_l1_batch(
        &mut self,
        block_result: vm::VmBlockResult,
        updates_manager: UpdatesManager,
        block_context: vm::vm_with_bootloader::DerivedBlockContext,
    ) {
        match self.actions.pop_action() {
            Some(SyncAction::SealBatch) => {}
            other => panic!(
                "State keeper requested to seal the batch, but the next action is {:?}",
                other
            ),
        };

        let mut storage = self.pool.access_storage_tagged("sync_layer").await;
        updates_manager
            .seal_l1_batch(
                &mut storage,
                self.current_miniblock_number,
                self.current_l1_batch_number,
                block_result,
                block_context,
                self.l2_erc20_bridge_addr,
            )
            .await;

        vlog::info!("Batch {} is sealed", self.current_l1_batch_number);

        // Mimic the metric emitted by the main node to reuse existing grafana charts.
        metrics::gauge!(
            "server.block_number",
            self.current_l1_batch_number.0 as f64,
            "stage" =>  "sealed"
        );

        self.current_miniblock_number += 1; // Due to fictive miniblock being sealed.
        self.current_l1_batch_number += 1;
    }

    async fn load_previous_batch_version_id(&mut self) -> Option<ProtocolVersionId> {
        let mut storage = self.pool.access_storage().await;
        storage
            .blocks_dal()
            .get_batch_protocol_version_id(self.current_l1_batch_number - 1)
            .await
    }

    async fn load_upgrade_tx(
        &mut self,
        _version_id: ProtocolVersionId,
    ) -> Option<ProtocolUpgradeTx> {
        // External node will fetch upgrade tx from the main node.
        None
    }
}

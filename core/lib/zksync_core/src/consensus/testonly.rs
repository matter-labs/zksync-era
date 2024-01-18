//! Utilities for testing the consensus module.
use anyhow::Context as _;
use rand::Rng;
use zksync_concurrency::{ctx, error::Wrap as _, scope, sync, time};
use zksync_consensus_roles::validator;
use zksync_contracts::{BaseSystemContractsHashes, SystemContractCode};
use zksync_dal::ConnectionPool;
use zksync_types::{
    api, block::MiniblockHasher, Address, L1BatchNumber, L2ChainId, MiniblockNumber,
    ProtocolVersionId, H256,
};

use crate::{
    consensus::{
        storage::{BlockStore, CtxStorage},
        Store,
    },
    genesis::{ensure_genesis_state, GenesisParams},
    state_keeper::{
        seal_criteria::NoopSealer, tests::MockBatchExecutorBuilder, MiniblockSealer,
        ZkSyncStateKeeper,
    },
    sync_layer::{
        sync_action::{ActionQueue, ActionQueueSender, SyncAction},
        ExternalIO, MainNodeClient, SyncState,
    },
    utils::testonly::{create_l1_batch_metadata, create_l2_transaction},
};

#[derive(Debug, Default)]
pub(crate) struct MockMainNodeClient {
    prev_miniblock_hash: H256,
    l2_blocks: Vec<api::en::SyncBlock>,
}

impl MockMainNodeClient {
    /// `miniblock_count` doesn't include a fictive miniblock. Returns hashes of generated transactions.
    pub fn push_l1_batch(&mut self, miniblock_count: u32) -> Vec<H256> {
        let l1_batch_number = self
            .l2_blocks
            .last()
            .map_or(L1BatchNumber(0), |block| block.l1_batch_number + 1);
        let number_offset = self.l2_blocks.len() as u32;

        let mut tx_hashes = vec![];
        let l2_blocks = (0..=miniblock_count).map(|number| {
            let is_fictive = number == miniblock_count;
            let number = number + number_offset;
            let mut hasher = MiniblockHasher::new(
                MiniblockNumber(number),
                number.into(),
                self.prev_miniblock_hash,
            );

            let transactions = if is_fictive {
                vec![]
            } else {
                let transaction = create_l2_transaction(10, 100);
                tx_hashes.push(transaction.hash());
                hasher.push_tx_hash(transaction.hash());
                vec![transaction.into()]
            };
            let miniblock_hash = hasher.finalize(if number == 0 {
                ProtocolVersionId::Version0 // The genesis block always uses the legacy hashing mode
            } else {
                ProtocolVersionId::latest()
            });
            self.prev_miniblock_hash = miniblock_hash;

            api::en::SyncBlock {
                number: MiniblockNumber(number),
                l1_batch_number,
                last_in_batch: is_fictive,
                timestamp: number.into(),
                l1_gas_price: 2,
                l2_fair_gas_price: 3,
                base_system_contracts_hashes: BaseSystemContractsHashes::default(),
                operator_address: Address::repeat_byte(2),
                transactions: Some(transactions),
                virtual_blocks: Some(!is_fictive as u32),
                hash: Some(miniblock_hash),
                protocol_version: ProtocolVersionId::latest(),
            }
        });

        self.l2_blocks.extend(l2_blocks);
        tx_hashes
    }
}

#[async_trait::async_trait]
impl MainNodeClient for MockMainNodeClient {
    async fn fetch_system_contract_by_hash(
        &self,
        _hash: H256,
    ) -> anyhow::Result<SystemContractCode> {
        anyhow::bail!("Not implemented");
    }

    async fn fetch_genesis_contract_bytecode(
        &self,
        _address: Address,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        anyhow::bail!("Not implemented");
    }

    async fn fetch_protocol_version(
        &self,
        _protocol_version: ProtocolVersionId,
    ) -> anyhow::Result<api::ProtocolVersion> {
        anyhow::bail!("Not implemented");
    }

    async fn fetch_genesis_l1_batch_hash(&self) -> anyhow::Result<H256> {
        anyhow::bail!("Not implemented");
    }

    async fn fetch_l2_block_number(&self) -> anyhow::Result<MiniblockNumber> {
        if let Some(number) = self.l2_blocks.len().checked_sub(1) {
            Ok(MiniblockNumber(number as u32))
        } else {
            anyhow::bail!("Not implemented");
        }
    }

    async fn fetch_l2_block(
        &self,
        number: MiniblockNumber,
        with_transactions: bool,
    ) -> anyhow::Result<Option<api::en::SyncBlock>> {
        let Some(mut block) = self.l2_blocks.get(number.0 as usize).cloned() else {
            return Ok(None);
        };
        if !with_transactions {
            block.transactions = None;
        }
        Ok(Some(block))
    }
}

/// Fake StateKeeper for tests.
pub(super) struct StateKeeper {
    // Batch of the `last_block`.
    last_batch: L1BatchNumber,
    last_block: MiniblockNumber,
    // timestamp of the last block.
    last_timestamp: u64,
    batch_sealed: bool,

    fee_per_gas: u64,
    gas_per_pubdata: u32,
    operator_address: Address,

    pub(super) actions_sender: ActionQueueSender,
    pub(super) pool: ConnectionPool,
}

/// Fake StateKeeper task to be executed in the background.
pub(super) struct StateKeeperRunner {
    actions_queue: ActionQueue,
    operator_address: Address,
    pool: ConnectionPool,
}

impl StateKeeper {
    /// Constructs and initializes a new `StateKeeper`.
    /// Caller has to run `StateKeeperRunner.run()` task in the background.
    pub async fn new(
        pool: ConnectionPool,
        operator_address: Address,
    ) -> anyhow::Result<(Self, StateKeeperRunner)> {
        // ensure genesis
        let mut storage = pool.access_storage().await.context("access_storage()")?;
        if storage
            .blocks_dal()
            .is_genesis_needed()
            .await
            .context("is_genesis_needed()")?
        {
            let mut params = GenesisParams::mock();
            params.first_validator = operator_address;
            ensure_genesis_state(&mut storage, L2ChainId::default(), &params)
                .await
                .context("ensure_genesis_state()")?;
        }

        let last_l1_batch_number = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .context("get_sealed_l1_batch_number()")?
            .context("no L1 batches in storage")?;
        let last_miniblock_header = storage
            .blocks_dal()
            .get_last_sealed_miniblock_header()
            .await
            .context("get_last_sealed_miniblock_header()")?
            .context("no miniblocks in storage")?;

        let pending_batch = storage
            .blocks_dal()
            .pending_batch_exists()
            .await
            .context("pending_batch_exists()")?;
        let (actions_sender, actions_queue) = ActionQueue::new();
        Ok((
            Self {
                last_batch: last_l1_batch_number + if pending_batch { 1 } else { 0 },
                last_block: last_miniblock_header.number,
                last_timestamp: last_miniblock_header.timestamp,
                batch_sealed: !pending_batch,
                fee_per_gas: 10,
                gas_per_pubdata: 100,
                operator_address,
                actions_sender,
                pool: pool.clone(),
            },
            StateKeeperRunner {
                operator_address,
                actions_queue,
                pool: pool.clone(),
            },
        ))
    }

    fn open_block(&mut self) -> SyncAction {
        if self.batch_sealed {
            self.last_batch += 1;
            self.last_block += 1;
            self.last_timestamp += 5;
            self.batch_sealed = false;
            SyncAction::OpenBatch {
                number: self.last_batch,
                timestamp: self.last_timestamp,
                l1_gas_price: 2,
                l2_fair_gas_price: 3,
                operator_address: self.operator_address,
                protocol_version: ProtocolVersionId::latest(),
                first_miniblock_info: (self.last_block, 1),
            }
        } else {
            self.last_block += 1;
            self.last_timestamp += 2;
            SyncAction::Miniblock {
                number: self.last_block,
                timestamp: self.last_timestamp,
                virtual_blocks: 0,
            }
        }
    }

    /// Pushes a new miniblock with `transactions` transactions to the `StateKeeper`.
    pub async fn push_block(&mut self, transactions: usize) {
        assert!(transactions > 0);
        let mut actions = vec![self.open_block()];
        for _ in 0..transactions {
            let tx = create_l2_transaction(self.fee_per_gas, self.gas_per_pubdata);
            actions.push(SyncAction::Tx(Box::new(tx.into())));
        }
        actions.push(SyncAction::SealMiniblock);
        self.actions_sender.push_actions(actions).await;
    }

    /// Pushes `SealBatch` command to the `StateKeeper`.
    pub async fn seal_batch(&mut self) {
        // Each batch ends with an empty block (aka fictive block).
        let mut actions = vec![self.open_block()];
        actions.push(SyncAction::SealBatch { virtual_blocks: 0 });
        self.actions_sender.push_actions(actions).await;
        self.batch_sealed = true;
    }

    /// Pushes `count` random miniblocks to the StateKeeper.
    pub async fn push_random_blocks(&mut self, rng: &mut impl Rng, count: usize) {
        for _ in 0..count {
            // 20% chance to seal an L1 batch.
            // `seal_batch()` also produces a (fictive) block.
            if rng.gen_range(0..100) < 20 {
                self.seal_batch().await;
            } else {
                self.push_block(rng.gen_range(3..8)).await;
            }
        }
    }

    /// Last block that has been pushed to the `StateKeeper` via `ActionQueue`.
    /// It might NOT be present in storage yet.
    pub fn last_block(&self) -> validator::BlockNumber {
        validator::BlockNumber(self.last_block.0 as u64)
    }

    /// Creates a new `BlockStore` for the underlying `ConnectionPool`.
    pub fn store(&self) -> BlockStore {
        Store::new(self.pool.clone(), self.operator_address).into_block_store()
    }

    // Wait for all pushed miniblocks to be produced.
    pub async fn wait_for_miniblocks(&self, ctx: &ctx::Ctx) -> ctx::Result<()> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(100);
        loop {
            let mut storage = CtxStorage::access(ctx, &self.pool).await.wrap("access()")?;
            if storage
                .payload(ctx, self.last_block(), self.operator_address)
                .await
                .wrap("storage.payload()")?
                .is_some()
            {
                return Ok(());
            }
            ctx.sleep(POLL_INTERVAL).await?;
        }
    }
}

/// Waits for L1 batches to be sealed and then populates them with mock metadata.
async fn run_mock_metadata_calculator(ctx: &ctx::Ctx, pool: &ConnectionPool) -> anyhow::Result<()> {
    const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(100);
    let mut n = {
        let mut storage = pool.access_storage().await.context("access_storage()")?;
        storage
            .blocks_dal()
            .get_last_l1_batch_number_with_metadata()
            .await
            .context("get_last_l1_batch_number_with_metadata()")?
            .context("no L1 batches in Postgres")?
    };
    while let Ok(()) = ctx.sleep(POLL_INTERVAL).await {
        let mut storage = pool.access_storage().await.context("access_storage()")?;
        let last = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .context("get_sealed_l1_batch_number()")?
            .context("no L1 batches in Postgres")?;

        while n < last {
            n += 1;
            let metadata = create_l1_batch_metadata(n.0);
            storage
                .blocks_dal()
                .save_l1_batch_metadata(n, &metadata, H256::zero(), false)
                .await
                .context("save_l1_batch_metadata()")?;
        }
    }
    Ok(())
}

impl StateKeeperRunner {
    /// Executes the StateKeeper task.
    pub async fn run(self, ctx: &ctx::Ctx) -> anyhow::Result<()> {
        scope::run!(ctx, |ctx, s| async {
            let (stop_sender, stop_receiver) = sync::watch::channel(false);
            let (miniblock_sealer, miniblock_sealer_handle) =
                MiniblockSealer::new(self.pool.clone(), 5);
            let io = ExternalIO::new(
                miniblock_sealer_handle,
                self.pool.clone(),
                self.actions_queue,
                SyncState::new(),
                Box::<MockMainNodeClient>::default(),
                self.operator_address,
                u32::MAX,
                L2ChainId::default(),
            )
            .await;
            s.spawn_bg(miniblock_sealer.run());
            s.spawn_bg(run_mock_metadata_calculator(ctx, &self.pool));
            s.spawn_bg(
                ZkSyncStateKeeper::new(
                    stop_receiver,
                    Box::new(io),
                    Box::new(MockBatchExecutorBuilder),
                    Box::new(NoopSealer),
                )
                .run(),
            );
            ctx.canceled().await;
            stop_sender.send_replace(true);
            Ok(())
        })
        .await
    }
}

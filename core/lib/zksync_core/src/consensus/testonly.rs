use anyhow::Context as _;
use rand::Rng;
use zksync_concurrency::{ctx, scope, sync, time};
use zksync_consensus_roles::validator;
use zksync_contracts::{BaseSystemContractsHashes, SystemContractCode};
use zksync_dal::ConnectionPool;
use zksync_types::{
    api, block::MiniblockHasher, Address, L1BatchNumber, L2ChainId, MiniblockNumber,
    ProtocolVersionId, H256,
};

use crate::{
    genesis::{ensure_genesis_state, GenesisParams},
    state_keeper::{
        tests::{create_l1_batch_metadata, create_l2_transaction, MockBatchExecutorBuilder},
        MiniblockSealer, ZkSyncStateKeeper,
    },
    sync_layer::{
        sync_action::{ActionQueue, ActionQueueSender, SyncAction},
        ExternalIO, MainNodeClient, SyncState,
    },
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
                consensus: None,
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

pub(crate) struct StateKeeperHandle {
    next_batch: L1BatchNumber,
    next_block: MiniblockNumber,
    next_timestamp: u64,
    batch_sealed: bool,

    fee_per_gas: u64,
    gas_per_pubdata: u32,
    operator_address: Address,

    actions_sender: ActionQueueSender,
}

pub(crate) struct StateKeeperRunner {
    actions_queue: ActionQueue,
    operator_address: Address,
}

impl StateKeeperHandle {
    pub fn new(operator_address: Address) -> (Self, StateKeeperRunner) {
        let (actions_sender, actions_queue) = ActionQueue::new();
        (
            Self {
                next_batch: L1BatchNumber(1),
                next_block: MiniblockNumber(1),
                next_timestamp: 124356,
                batch_sealed: true,
                fee_per_gas: 10,
                gas_per_pubdata: 100,
                operator_address,
                actions_sender,
            },
            StateKeeperRunner {
                operator_address,
                actions_queue,
            },
        )
    }

    fn open_block(&mut self) -> SyncAction {
        if self.batch_sealed {
            let action = SyncAction::OpenBatch {
                number: self.next_batch,
                timestamp: self.next_timestamp,
                l1_gas_price: 2,
                l2_fair_gas_price: 3,
                operator_address: self.operator_address,
                protocol_version: ProtocolVersionId::latest(),
                first_miniblock_info: (self.next_block, 1),
            };
            self.next_batch += 1;
            self.next_block += 1;
            self.next_timestamp += 5;
            self.batch_sealed = false;
            action
        } else {
            let action = SyncAction::Miniblock {
                number: self.next_block,
                timestamp: self.next_timestamp,
                virtual_blocks: 0,
            };
            self.next_block += 1;
            self.next_timestamp += 2;
            action
        }
    }

    pub async fn push_block(&mut self, transactions: usize) {
        assert!(transactions > 0);
        let mut actions = vec![self.open_block()];
        for _ in 0..transactions {
            let tx = create_l2_transaction(self.fee_per_gas, self.gas_per_pubdata);
            actions.push(SyncAction::Tx(Box::new(tx.into())));
        }
        actions.push(SyncAction::SealMiniblock(None));
        self.actions_sender.push_actions(actions).await;
    }

    pub async fn seal_batch(&mut self) {
        // Each batch ends with an empty block (aka fictive block).
        let mut actions = vec![self.open_block()];
        actions.push(SyncAction::SealBatch {
            virtual_blocks: 0,
            consensus: None,
        });
        self.actions_sender.push_actions(actions).await;
        self.batch_sealed = true;
    }

    pub async fn push_random_blocks(&mut self, rng: &mut impl Rng, count: usize) {
        for _ in 0..count {
            // 20% chance to seal an L1 batch.
            // seal_batch() also produces a (fictive) block.
            if rng.gen_range(0..100) < 20 {
                self.seal_batch().await;
            } else {
                self.push_block(rng.gen_range(3..8)).await;
            }
        }
    }

    // Wait for all pushed miniblocks to be produced.
    pub async fn sync(&self, ctx: &ctx::Ctx, pool: &ConnectionPool) -> anyhow::Result<()> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(100);
        loop {
            let mut storage = pool.access_storage().await.context("access_storage()")?;
            let head = storage
                .blocks_dal()
                .get_sealed_miniblock_number()
                .await
                .context("get_sealed_miniblock_number()")?;
            if head.0 >= self.next_block.0 - 1 {
                return Ok(());
            }
            ctx.sleep(POLL_INTERVAL).await?;
        }
    }

    // Wait for all pushed miniblocks to have consensus certificate.
    pub async fn sync_consensus(
        &self,
        ctx: &ctx::Ctx,
        pool: &ConnectionPool,
    ) -> anyhow::Result<()> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(100);
        loop {
            let mut storage = pool.access_storage().await.context("access_storage()")?;
            if let Some(head) = storage
                .blocks_dal()
                .get_last_miniblock_number_with_consensus_fields()
                .await
                .context("get_last_miniblock_number_with_consensus_fields()")?
            {
                if head.0 >= self.next_block.0 - 1 {
                    return Ok(());
                }
            }
            ctx.sleep(POLL_INTERVAL).await?;
        }
    }

    /// Validate consensus certificates for all expected miniblocks.
    pub async fn validate_consensus(
        &self,
        ctx: &ctx::Ctx,
        pool: &ConnectionPool,
        genesis: validator::BlockNumber,
        validators: &validator::ValidatorSet,
    ) -> anyhow::Result<()> {
        let mut storage = super::storage::storage(ctx, pool)
            .await
            .context("storage()")?;
        for i in genesis.0..self.next_block.0 as u64 {
            let block = storage
                .fetch_block(ctx, validator::BlockNumber(i), self.operator_address)
                .await?
                .with_context(|| format!("missing block {i}"))?;
            block.validate(validators, 1).unwrap();
        }
        Ok(())
    }
}

// Waits for L1 batches to be sealed and then populates them with mock metadata.
async fn run_mock_metadata_calculator(ctx: &ctx::Ctx, pool: ConnectionPool) -> anyhow::Result<()> {
    const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(100);
    let mut n = L1BatchNumber(1);
    while let Ok(()) = ctx.sleep(POLL_INTERVAL).await {
        let mut storage = pool.access_storage().await.context("access_storage()")?;
        let last = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .context("get_sealed_l1_batch_number()")?;

        while n <= last {
            let metadata = create_l1_batch_metadata(n.0);
            storage
                .blocks_dal()
                .save_l1_batch_metadata(n, &metadata, H256::zero(), false)
                .await
                .context("save_l1_batch_metadata()")?;
            n += 1;
        }
    }
    Ok(())
}

impl StateKeeperRunner {
    pub async fn run(self, ctx: &ctx::Ctx, pool: &ConnectionPool) -> anyhow::Result<()> {
        scope::run!(ctx, |ctx, s| async {
            let mut storage = pool.access_storage().await.context("access_storage()")?;
            // ensure genesis
            if storage
                .blocks_dal()
                .is_genesis_needed()
                .await
                .context("is_genesis_needed()")?
            {
                let mut params = GenesisParams::mock();
                params.first_validator = self.operator_address;
                ensure_genesis_state(&mut storage, L2ChainId::default(), &params)
                    .await
                    .context("ensure_genesis_state()")?;
            }
            let (stop_sender, stop_receiver) = sync::watch::channel(false);
            let (miniblock_sealer, miniblock_sealer_handle) = MiniblockSealer::new(pool.clone(), 5);
            let io = ExternalIO::new(
                miniblock_sealer_handle,
                pool.clone(),
                self.actions_queue,
                SyncState::new(),
                Box::<MockMainNodeClient>::default(),
                self.operator_address,
                u32::MAX,
                L2ChainId::default(),
            )
            .await;
            s.spawn_bg(miniblock_sealer.run());
            s.spawn_bg(run_mock_metadata_calculator(ctx, pool.clone()));
            s.spawn_bg(
                ZkSyncStateKeeper::without_sealer(
                    stop_receiver,
                    Box::new(io),
                    Box::new(MockBatchExecutorBuilder),
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

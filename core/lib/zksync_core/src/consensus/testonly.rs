//! Utilities for testing the consensus module.
use std::{collections::HashMap, sync::Arc};

use anyhow::Context as _;
use rand::{
    distributions::{Distribution, Standard},
    Rng,
};
use zksync_concurrency::{ctx, error::Wrap as _, scope, sync, time};
use zksync_consensus_roles::{node, validator};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::ConnectionPool;
use zksync_types::{
    api, block::MiniblockHasher, snapshots::SnapshotRecoveryStatus, Address, L1BatchNumber,
    L2ChainId, MiniblockNumber, ProtocolVersionId, H256, U256,
};
use zksync_web3_decl::error::{EnrichedClientError, EnrichedClientResult};

use crate::{
    consensus::{
        config::Config,
        storage::{BlockStore, CtxStorage},
        Store,
    },
    genesis::{ensure_genesis_state, GenesisParams},
    state_keeper::{
        seal_criteria::NoopSealer, tests::MockBatchExecutor, MiniblockSealer, ZkSyncStateKeeper,
    },
    sync_layer::{
        sync_action::{ActionQueue, ActionQueueSender, SyncAction},
        ExternalIO, MainNodeClient, SyncState,
    },
    utils::testonly::{create_l1_batch_metadata, create_l2_transaction},
};

fn make_addr<R: Rng + ?Sized>(rng: &mut R) -> std::net::SocketAddr {
    std::net::SocketAddr::new(std::net::IpAddr::from(rng.gen::<[u8; 16]>()), rng.gen())
}

fn make_node_key<R: Rng + ?Sized>(rng: &mut R) -> node::PublicKey {
    rng.gen::<node::SecretKey>().public()
}

impl Distribution<Config> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Config {
        Config {
            server_addr: make_addr(rng),
            public_addr: make_addr(rng),
            validators: rng.gen(),
            max_payload_size: usize::MAX,
            gossip_dynamic_inbound_limit: rng.gen(),
            gossip_static_inbound: (0..3).map(|_| make_node_key(rng)).collect(),
            gossip_static_outbound: (0..5)
                .map(|_| (make_node_key(rng), make_addr(rng)))
                .collect(),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct MockMainNodeClient {
    prev_miniblock_hash: H256,
    l2_blocks: Vec<api::en::SyncBlock>,
    block_number_offset: u32,
    protocol_versions: HashMap<u16, api::ProtocolVersion>,
    system_contracts: HashMap<H256, Vec<u8>>,
}

impl MockMainNodeClient {
    pub fn for_snapshot_recovery(snapshot: &SnapshotRecoveryStatus) -> Self {
        // This block may be requested during node initialization
        let last_miniblock_in_snapshot_batch = api::en::SyncBlock {
            number: snapshot.miniblock_number,
            l1_batch_number: snapshot.l1_batch_number,
            last_in_batch: true,
            timestamp: snapshot.miniblock_timestamp,
            l1_gas_price: 2,
            l2_fair_gas_price: 3,
            fair_pubdata_price: Some(24),
            base_system_contracts_hashes: BaseSystemContractsHashes::default(),
            operator_address: Address::repeat_byte(2),
            transactions: Some(vec![]),
            virtual_blocks: Some(0),
            hash: Some(snapshot.miniblock_hash),
            protocol_version: ProtocolVersionId::latest(),
        };

        Self {
            prev_miniblock_hash: snapshot.miniblock_hash,
            l2_blocks: vec![last_miniblock_in_snapshot_batch],
            block_number_offset: snapshot.miniblock_number.0,
            ..Self::default()
        }
    }

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
                l1_gas_price: U256::from(2),
                l2_fair_gas_price: U256::from(3),
                fair_pubdata_price: Some(U256::from(24)),
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

    pub fn insert_protocol_version(&mut self, version: api::ProtocolVersion) {
        self.system_contracts
            .insert(version.base_system_contracts.bootloader, vec![]);
        self.system_contracts
            .insert(version.base_system_contracts.default_aa, vec![]);
        self.protocol_versions.insert(version.version_id, version);
    }
}

#[async_trait::async_trait]
impl MainNodeClient for MockMainNodeClient {
    async fn fetch_system_contract_by_hash(
        &self,
        hash: H256,
    ) -> EnrichedClientResult<Option<Vec<u8>>> {
        Ok(self.system_contracts.get(&hash).cloned())
    }

    async fn fetch_genesis_contract_bytecode(
        &self,
        _address: Address,
    ) -> EnrichedClientResult<Option<Vec<u8>>> {
        Err(EnrichedClientError::custom(
            "not implemented",
            "fetch_genesis_contract_bytecode",
        ))
    }

    async fn fetch_protocol_version(
        &self,
        protocol_version: ProtocolVersionId,
    ) -> EnrichedClientResult<Option<api::ProtocolVersion>> {
        let protocol_version = protocol_version as u16;
        Ok(self.protocol_versions.get(&protocol_version).cloned())
    }

    async fn fetch_genesis_l1_batch_hash(&self) -> EnrichedClientResult<H256> {
        Err(EnrichedClientError::custom(
            "not implemented",
            "fetch_genesis_l1_batch_hash",
        ))
    }

    async fn fetch_l2_block_number(&self) -> EnrichedClientResult<MiniblockNumber> {
        if let Some(number) = self.l2_blocks.len().checked_sub(1) {
            Ok(MiniblockNumber(number as u32))
        } else {
            Err(EnrichedClientError::custom(
                "not implemented",
                "fetch_l2_block_number",
            ))
        }
    }

    async fn fetch_l2_block(
        &self,
        number: MiniblockNumber,
        with_transactions: bool,
    ) -> EnrichedClientResult<Option<api::en::SyncBlock>> {
        let Some(block_index) = number.0.checked_sub(self.block_number_offset) else {
            return Ok(None);
        };
        let Some(mut block) = self.l2_blocks.get(block_index as usize).cloned() else {
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
    gas_per_pubdata: u64,

    pub(super) actions_sender: ActionQueueSender,
    pub(super) pool: ConnectionPool,
}

/// Fake StateKeeper task to be executed in the background.
pub(super) struct StateKeeperRunner {
    actions_queue: ActionQueue,
    pool: ConnectionPool,
}

impl StateKeeper {
    /// Constructs and initializes a new `StateKeeper`.
    /// Caller has to run `StateKeeperRunner.run()` task in the background.
    pub async fn new(pool: ConnectionPool) -> anyhow::Result<(Self, StateKeeperRunner)> {
        // ensure genesis
        let mut storage = pool.access_storage().await.context("access_storage()")?;
        if storage
            .blocks_dal()
            .is_genesis_needed()
            .await
            .context("is_genesis_needed()")?
        {
            ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
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
                actions_sender,
                pool: pool.clone(),
            },
            StateKeeperRunner {
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
                l1_gas_price: U256::from(2),
                l2_fair_gas_price: U256::from(3),
                fair_pubdata_price: Some(U256::from(24)),
                operator_address: GenesisParams::mock().first_validator,
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
        Store::new(self.pool.clone()).into_block_store()
    }

    // Wait for all pushed miniblocks to be produced.
    pub async fn wait_for_miniblocks(&self, ctx: &ctx::Ctx) -> ctx::Result<()> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(100);

        loop {
            let mut storage = CtxStorage::access(ctx, &self.pool).await.wrap("access()")?;
            if storage
                .payload(ctx, self.last_block())
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
                .save_l1_batch_tree_data(n, &metadata.tree_data())
                .await
                .context("save_l1_batch_tree_data()")?;
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
                SyncState::default(),
                Box::<MockMainNodeClient>::default(),
                Address::repeat_byte(11),
                u32::MAX,
                L2ChainId::default(),
            )
            .await?;
            s.spawn_bg(miniblock_sealer.run());
            s.spawn_bg(run_mock_metadata_calculator(ctx, &self.pool));
            s.spawn_bg(
                ZkSyncStateKeeper::new(
                    stop_receiver,
                    Box::new(io),
                    Box::new(MockBatchExecutor),
                    Arc::new(NoopSealer),
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

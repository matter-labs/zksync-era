//! Utilities for testing the consensus module.

use std::{collections::HashMap, sync::Arc};

use anyhow::Context as _;
use rand::Rng;
use zksync_concurrency::{ctx, error::Wrap as _, scope, sync};
use zksync_config::{configs, GenesisConfig};
use zksync_consensus_roles::validator;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{CoreDal, DalError};
use zksync_node_genesis::{mock_genesis_config, GenesisParams};
use zksync_node_test_utils::{create_l1_batch_metadata, create_l2_transaction};
use zksync_types::{
    api, snapshots::SnapshotRecoveryStatus, Address, L1BatchNumber, L2BlockNumber, L2ChainId,
    ProtocolVersionId, H256,
};
use zksync_web3_decl::{
    client::{Client, DynClient, L2},
    error::{EnrichedClientError, EnrichedClientResult},
};

use crate::{
    api_server::web3::{state::InternalApiConfig, tests::spawn_http_server},
    consensus::{fetcher::P2PConfig, Fetcher, Store},
    state_keeper::{
        io::{IoCursor, L1BatchParams, L2BlockParams},
        seal_criteria::NoopSealer,
        tests::MockBatchExecutor,
        OutputHandler, StateKeeperPersistence, ZkSyncStateKeeper,
    },
    sync_layer::{
        fetcher::FetchedTransaction,
        sync_action::{ActionQueue, ActionQueueSender, SyncAction},
        ExternalIO, MainNodeClient, SyncState,
    },
};

#[derive(Debug, Default)]
pub(crate) struct MockMainNodeClient {
    l2_blocks: Vec<api::en::SyncBlock>,
    block_number_offset: u32,
    protocol_versions: HashMap<u16, api::ProtocolVersion>,
    system_contracts: HashMap<H256, Vec<u8>>,
}

impl MockMainNodeClient {
    pub fn for_snapshot_recovery(snapshot: &SnapshotRecoveryStatus) -> Self {
        // This block may be requested during node initialization
        let last_l2_block_in_snapshot_batch = api::en::SyncBlock {
            number: snapshot.l2_block_number,
            l1_batch_number: snapshot.l1_batch_number,
            last_in_batch: true,
            timestamp: snapshot.l2_block_timestamp,
            l1_gas_price: 2,
            l2_fair_gas_price: 3,
            fair_pubdata_price: Some(24),
            base_system_contracts_hashes: BaseSystemContractsHashes::default(),
            operator_address: Address::repeat_byte(2),
            transactions: Some(vec![]),
            virtual_blocks: Some(0),
            hash: Some(snapshot.l2_block_hash),
            protocol_version: ProtocolVersionId::latest(),
        };

        Self {
            l2_blocks: vec![last_l2_block_in_snapshot_batch],
            block_number_offset: snapshot.l2_block_number.0,
            ..Self::default()
        }
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

    async fn fetch_l2_block_number(&self) -> EnrichedClientResult<L2BlockNumber> {
        if let Some(number) = self.l2_blocks.len().checked_sub(1) {
            Ok(L2BlockNumber(number as u32))
        } else {
            Err(EnrichedClientError::custom(
                "not implemented",
                "fetch_l2_block_number",
            ))
        }
    }

    async fn fetch_l2_block(
        &self,
        number: L2BlockNumber,
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

    async fn fetch_consensus_genesis(
        &self,
    ) -> EnrichedClientResult<Option<api::en::ConsensusGenesis>> {
        unimplemented!()
    }

    async fn fetch_genesis_config(&self) -> EnrichedClientResult<GenesisConfig> {
        Ok(mock_genesis_config())
    }
}

/// Fake StateKeeper for tests.
pub(super) struct StateKeeper {
    // Batch of the `last_block`.
    last_batch: L1BatchNumber,
    last_block: L2BlockNumber,
    // timestamp of the last block.
    last_timestamp: u64,
    batch_sealed: bool,

    fee_per_gas: u64,
    gas_per_pubdata: u64,

    actions_sender: ActionQueueSender,
    addr: sync::watch::Receiver<Option<std::net::SocketAddr>>,
    store: Store,
}

/// Fake StateKeeper task to be executed in the background.
pub(super) struct StateKeeperRunner {
    actions_queue: ActionQueue,
    store: Store,
    addr: sync::watch::Sender<Option<std::net::SocketAddr>>,
}

impl StateKeeper {
    /// Constructs and initializes a new `StateKeeper`.
    /// Caller has to run `StateKeeperRunner.run()` task in the background.
    pub async fn new(ctx: &ctx::Ctx, store: Store) -> ctx::Result<(Self, StateKeeperRunner)> {
        let mut conn = store.access(ctx).await.wrap("access()")?;
        let cursor = ctx
            .wait(IoCursor::for_fetcher(&mut conn.0))
            .await?
            .context("IoCursor::new()")?;
        let pending_batch = ctx
            .wait(conn.0.blocks_dal().pending_batch_exists())
            .await?
            .context("pending_batch_exists()")?;
        let (actions_sender, actions_queue) = ActionQueue::new();
        let addr = sync::watch::channel(None).0;
        Ok((
            Self {
                last_batch: cursor.l1_batch,
                last_block: cursor.next_l2_block - 1,
                last_timestamp: cursor.prev_l2_block_timestamp,
                batch_sealed: !pending_batch,
                fee_per_gas: 10,
                gas_per_pubdata: 100,
                actions_sender,
                addr: addr.subscribe(),
                store: store.clone(),
            },
            StateKeeperRunner {
                actions_queue,
                store: store.clone(),
                addr,
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
                params: L1BatchParams {
                    protocol_version: ProtocolVersionId::latest(),
                    validation_computational_gas_limit: u32::MAX,
                    operator_address: GenesisParams::mock().config().fee_account,
                    fee_input: Default::default(),
                    first_l2_block: L2BlockParams {
                        timestamp: self.last_timestamp,
                        virtual_blocks: 1,
                    },
                },
                number: self.last_batch,
                first_l2_block_number: self.last_block,
            }
        } else {
            self.last_block += 1;
            self.last_timestamp += 2;
            SyncAction::L2Block {
                params: L2BlockParams {
                    timestamp: self.last_timestamp,
                    virtual_blocks: 0,
                },
                number: self.last_block,
            }
        }
    }

    /// Pushes a new L2 block with `transactions` transactions to the `StateKeeper`.
    pub async fn push_block(&mut self, transactions: usize) {
        assert!(transactions > 0);
        let mut actions = vec![self.open_block()];
        for _ in 0..transactions {
            let tx = create_l2_transaction(self.fee_per_gas, self.gas_per_pubdata);
            actions.push(FetchedTransaction::new(tx.into()).into());
        }
        actions.push(SyncAction::SealL2Block);
        self.actions_sender.push_actions(actions).await;
    }

    /// Pushes `SealBatch` command to the `StateKeeper`.
    pub async fn seal_batch(&mut self) {
        // Each batch ends with an empty block (aka fictive block).
        let mut actions = vec![self.open_block()];
        actions.push(SyncAction::SealBatch);
        self.actions_sender.push_actions(actions).await;
        self.batch_sealed = true;
    }

    /// Pushes `count` random L2 blocks to the StateKeeper.
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
        validator::BlockNumber(self.last_block.0.into())
    }

    /// Connects to the json RPC endpoint exposed by the state keeper.
    pub async fn connect(&self, ctx: &ctx::Ctx) -> ctx::Result<Box<DynClient<L2>>> {
        let addr = sync::wait_for(ctx, &mut self.addr.clone(), Option::is_some)
            .await?
            .unwrap();
        let client = Client::http(format!("http://{addr}/").parse().context("url")?)
            .context("json_rpc()")?
            .build();
        Ok(Box::new(client))
    }

    /// Runs the centralized fetcher.
    pub async fn run_centralized_fetcher(
        self,
        ctx: &ctx::Ctx,
        client: Box<DynClient<L2>>,
    ) -> anyhow::Result<()> {
        Fetcher {
            store: self.store,
            client,
            sync_state: SyncState::default(),
        }
        .run_centralized(ctx, self.actions_sender)
        .await
    }

    /// Runs the p2p fetcher.
    pub async fn run_p2p_fetcher(
        self,
        ctx: &ctx::Ctx,
        client: Box<DynClient<L2>>,
        cfg: P2PConfig,
    ) -> anyhow::Result<()> {
        Fetcher {
            store: self.store,
            client,
            sync_state: SyncState::default(),
        }
        .run_p2p(ctx, self.actions_sender, cfg)
        .await
    }
}

async fn calculate_mock_metadata(ctx: &ctx::Ctx, store: &Store) -> ctx::Result<()> {
    let mut conn = store.access(ctx).await.wrap("access()")?;
    let Some(last) = ctx
        .wait(conn.0.blocks_dal().get_sealed_l1_batch_number())
        .await?
        .map_err(DalError::generalize)?
    else {
        return Ok(());
    };
    let prev = ctx
        .wait(conn.0.blocks_dal().get_last_l1_batch_number_with_metadata())
        .await?
        .map_err(DalError::generalize)?;
    let mut first = match prev {
        Some(prev) => prev + 1,
        None => ctx
            .wait(conn.0.blocks_dal().get_earliest_l1_batch_number())
            .await?
            .map_err(DalError::generalize)?
            .context("batches disappeared")?,
    };
    while first <= last {
        let metadata = create_l1_batch_metadata(first.0);
        ctx.wait(
            conn.0
                .blocks_dal()
                .save_l1_batch_tree_data(first, &metadata.tree_data()),
        )
        .await?
        .context("save_l1_batch_tree_data()")?;
        first += 1;
    }
    Ok(())
}

impl StateKeeperRunner {
    /// Executes the StateKeeper task.
    pub async fn run(self, ctx: &ctx::Ctx) -> anyhow::Result<()> {
        let res = scope::run!(ctx, |ctx, s| async {
            let (stop_send, stop_recv) = sync::watch::channel(false);
            let (persistence, l2_block_sealer) =
                StateKeeperPersistence::new(self.store.0.clone(), Address::repeat_byte(11), 5);

            let io = ExternalIO::new(
                self.store.0.clone(),
                self.actions_queue,
                Box::<MockMainNodeClient>::default(),
                L2ChainId::default(),
            )
            .await?;
            s.spawn_bg(async {
                Ok(l2_block_sealer
                    .run()
                    .await
                    .context("l2_block_sealer.run()")?)
            });
            s.spawn_bg::<()>(async {
                loop {
                    calculate_mock_metadata(ctx, &self.store).await?;
                    // Sleep real time.
                    ctx.wait(tokio::time::sleep(tokio::time::Duration::from_millis(100)))
                        .await?;
                }
            });
            s.spawn_bg({
                let stop_recv = stop_recv.clone();
                async {
                    ZkSyncStateKeeper::new(
                        stop_recv,
                        Box::new(io),
                        Box::new(MockBatchExecutor),
                        OutputHandler::new(Box::new(persistence.with_tx_insertion())),
                        Arc::new(NoopSealer),
                    )
                    .run()
                    .await
                    .context("ZkSyncStateKeeper::run()")?;
                    Ok(())
                }
            });
            s.spawn_bg(async {
                // Spawn HTTP server.
                let cfg = InternalApiConfig::new(
                    &configs::api::Web3JsonRpcConfig::for_tests(),
                    &configs::contracts::ContractsConfig::for_tests(),
                    &configs::GenesisConfig::for_tests(),
                );
                let mut server = spawn_http_server(
                    cfg,
                    self.store.0.clone(),
                    Default::default(),
                    Arc::default(),
                    stop_recv,
                )
                .await;
                if let Ok(addr) = ctx.wait(server.wait_until_ready()).await {
                    self.addr.send_replace(Some(addr));
                    tracing::info!("API server ready!");
                }
                ctx.canceled().await;
                server.shutdown().await;
                Ok(())
            });
            ctx.canceled().await;
            stop_send.send_replace(true);
            Ok(())
        })
        .await;
        match res {
            Ok(()) | Err(ctx::Error::Canceled(_)) => Ok(()),
            Err(ctx::Error::Internal(err)) => Err(err),
        }
    }
}

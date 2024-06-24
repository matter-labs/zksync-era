//! Utilities for testing the consensus module.
use std::sync::Arc;

use anyhow::Context as _;
use rand::Rng;
use zksync_concurrency::{ctx, error::Wrap as _, scope, sync, time};
use zksync_config::{
    configs,
    configs::{
        chain::OperationsManagerConfig,
        consensus as config,
        database::{MerkleTreeConfig, MerkleTreeMode},
    },
};
use zksync_consensus_crypto::TextFmt as _;
use zksync_consensus_network as network;
use zksync_consensus_roles::validator;
use zksync_dal::{CoreDal, DalError};
use zksync_l1_contract_interface::i_executor::structures::StoredBatchInfo;
use zksync_metadata_calculator::{
    LazyAsyncTreeReader, MetadataCalculator, MetadataCalculatorConfig,
};
use zksync_node_api_server::web3::{state::InternalApiConfig, testonly::spawn_http_server};
use zksync_node_genesis::GenesisParams;
use zksync_node_sync::{
    fetcher::{FetchedTransaction, IoCursorExt as _},
    sync_action::{ActionQueue, ActionQueueSender, SyncAction},
    testonly::MockMainNodeClient,
    ExternalIO, MainNodeClient, SyncState,
};
use zksync_node_test_utils::{create_l1_batch_metadata, l1_batch_metadata_to_commitment_artifacts};
use zksync_state::RocksdbStorageOptions;
use zksync_state_keeper::{
    io::{IoCursor, L1BatchParams, L2BlockParams},
    seal_criteria::NoopSealer,
    testonly::{
        fund, l1_transaction, l2_transaction, test_batch_executor::MockReadStorageFactory,
        MockBatchExecutor,
    },
    AsyncRocksdbCache, MainBatchExecutor, OutputHandler, StateKeeperPersistence,
    TreeWritesPersistence, ZkSyncStateKeeper,
};
use zksync_test_account::Account;
use zksync_types::{
    fee_model::{BatchFeeInput, L1PeggedBatchFeeModelInput},
    Address, L1BatchNumber, L2BlockNumber, L2ChainId, PriorityOpId, ProtocolVersionId,
};
use zksync_web3_decl::client::{Client, DynClient, L2};

use crate::{
    batch::{L1BatchCommit, L1BatchWithWitness, LastBlockCommit},
    en, ConnectionPool,
};

/// Fake StateKeeper for tests.
pub(super) struct StateKeeper {
    protocol_version: ProtocolVersionId,
    // Batch of the `last_block`.
    last_batch: L1BatchNumber,
    last_block: L2BlockNumber,
    // timestamp of the last block.
    last_timestamp: u64,
    batch_sealed: bool,
    // test L2 account
    account: Account,
    next_priority_op: PriorityOpId,

    actions_sender: ActionQueueSender,
    sync_state: SyncState,
    addr: sync::watch::Receiver<Option<std::net::SocketAddr>>,
    pool: ConnectionPool,
    tree_reader: LazyAsyncTreeReader,
}

pub(super) fn config(cfg: &network::Config) -> (config::ConsensusConfig, config::ConsensusSecrets) {
    (
        config::ConsensusConfig {
            server_addr: *cfg.server_addr,
            public_addr: config::Host(cfg.public_addr.0.clone()),
            max_payload_size: usize::MAX,
            gossip_dynamic_inbound_limit: cfg.gossip.dynamic_inbound_limit,
            gossip_static_inbound: cfg
                .gossip
                .static_inbound
                .iter()
                .map(|k| config::NodePublicKey(k.encode()))
                .collect(),
            gossip_static_outbound: cfg
                .gossip
                .static_outbound
                .iter()
                .map(|(k, v)| (config::NodePublicKey(k.encode()), config::Host(v.0.clone())))
                .collect(),
            genesis_spec: cfg.validator_key.as_ref().map(|key| config::GenesisSpec {
                chain_id: L2ChainId::default(),
                protocol_version: config::ProtocolVersion(validator::ProtocolVersion::CURRENT.0),
                validators: vec![config::WeightedValidator {
                    key: config::ValidatorPublicKey(key.public().encode()),
                    weight: 1,
                }],
                leader: config::ValidatorPublicKey(key.public().encode()),
            }),
        },
        config::ConsensusSecrets {
            node_key: Some(config::NodeSecretKey(cfg.gossip.key.encode().into())),
            validator_key: cfg
                .validator_key
                .as_ref()
                .map(|k| config::ValidatorSecretKey(k.encode().into())),
        },
    )
}

/// Fake StateKeeper task to be executed in the background.
pub(super) struct StateKeeperRunner {
    actions_queue: ActionQueue,
    sync_state: SyncState,
    pool: ConnectionPool,

    addr: sync::watch::Sender<Option<std::net::SocketAddr>>,
    rocksdb_dir: tempfile::TempDir,
    metadata_calculator: MetadataCalculator,
    account: Account,
}

impl StateKeeper {
    /// Constructs and initializes a new `StateKeeper`.
    /// Caller has to run `StateKeeperRunner.run()` task in the background.
    pub async fn new(
        ctx: &ctx::Ctx,
        pool: ConnectionPool,
    ) -> ctx::Result<(Self, StateKeeperRunner)> {
        let mut conn = pool.connection(ctx).await.wrap("connection()")?;
        // We fetch the last protocol version from storage.
        // `protocol_version_id_by_timestamp` does a wrapping conversion to `i64`.
        let protocol_version = ctx
            .wait(
                conn.0
                    .protocol_versions_dal()
                    .protocol_version_id_by_timestamp(i64::MAX.try_into().unwrap()),
            )
            .await?
            .context("protocol_version_id_by_timestamp()")?;
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
        let sync_state = SyncState::default();

        let rocksdb_dir = tempfile::tempdir().context("tempdir()")?;
        let merkle_tree_config = MerkleTreeConfig {
            path: rocksdb_dir
                .path()
                .join("merkle_tree")
                .to_string_lossy()
                .into(),
            mode: MerkleTreeMode::Lightweight,
            ..Default::default()
        };
        let operation_manager_config = OperationsManagerConfig {
            delay_interval: 100, //`100ms`
        };
        let config =
            MetadataCalculatorConfig::for_main_node(&merkle_tree_config, &operation_manager_config);
        let metadata_calculator = MetadataCalculator::new(config, None, pool.0.clone())
            .await
            .context("MetadataCalculator::new()")?;
        let tree_reader = metadata_calculator.tree_reader();
        let account = Account::random();
        Ok((
            Self {
                protocol_version,
                last_batch: cursor.l1_batch,
                last_block: cursor.next_l2_block - 1,
                last_timestamp: cursor.prev_l2_block_timestamp,
                batch_sealed: !pending_batch,
                next_priority_op: PriorityOpId(1),
                actions_sender,
                sync_state: sync_state.clone(),
                addr: addr.subscribe(),
                pool: pool.clone(),
                tree_reader,
                account: account.clone(),
            },
            StateKeeperRunner {
                actions_queue,
                sync_state,
                pool: pool.clone(),
                addr,
                rocksdb_dir,
                metadata_calculator,
                account,
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
                    protocol_version: self.protocol_version,
                    validation_computational_gas_limit: u32::MAX,
                    operator_address: GenesisParams::mock().config().fee_account,
                    fee_input: BatchFeeInput::L1Pegged(L1PeggedBatchFeeModelInput {
                        fair_l2_gas_price: 10,
                        l1_gas_price: 100,
                    }),
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
    pub async fn push_random_block(&mut self, rng: &mut impl Rng) {
        let mut actions = vec![self.open_block()];
        for _ in 0..rng.gen_range(3..8) {
            let tx = match rng.gen() {
                true => l2_transaction(&mut self.account, 1_000_000),
                false => {
                    let tx = l1_transaction(&mut self.account, self.next_priority_op);
                    self.next_priority_op += 1;
                    tx
                }
            };
            actions.push(FetchedTransaction::new(tx).into());
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
                self.push_random_block(rng).await;
            }
        }
    }

    /// Last block that has been pushed to the `StateKeeper` via `ActionQueue`.
    /// It might NOT be present in storage yet.
    pub fn last_block(&self) -> validator::BlockNumber {
        validator::BlockNumber(self.last_block.0.into())
    }

    /// Last L1 batch that has been sealed and will have
    /// metadata computed eventually.
    pub fn last_sealed_batch(&self) -> L1BatchNumber {
        self.last_batch - (!self.batch_sealed) as u32
    }

    /// Loads a commitment to L1 batch directly from the database.
    // TODO: ideally, we should rather fake fetching it from Ethereum.
    // We can use `zksync_eth_client::clients::MockEthereum` for that,
    // which implements `EthInterface`. It should be enough to use
    // `MockEthereum.with_call_handler()`.
    pub async fn load_batch_commit(
        &self,
        ctx: &ctx::Ctx,
        number: L1BatchNumber,
    ) -> ctx::Result<L1BatchCommit> {
        // TODO: we should mock the `eth_sender` as well.
        let mut conn = self.pool.connection(ctx).await?;
        let this = conn.batch(ctx, number).await?.context("missing batch")?;
        let prev = conn
            .batch(ctx, number - 1)
            .await?
            .context("missing batch")?;
        Ok(L1BatchCommit {
            number,
            this_batch: LastBlockCommit {
                info: StoredBatchInfo::from(&this).hash(),
            },
            prev_batch: LastBlockCommit {
                info: StoredBatchInfo::from(&prev).hash(),
            },
        })
    }

    /// Loads an `L1BatchWithWitness`.
    pub async fn load_batch_with_witness(
        &self,
        ctx: &ctx::Ctx,
        n: L1BatchNumber,
    ) -> ctx::Result<L1BatchWithWitness> {
        L1BatchWithWitness::load(ctx, n, &self.pool, &self.tree_reader).await
    }

    /// Connects to the json RPC endpoint exposed by the state keeper.
    pub async fn connect(&self, ctx: &ctx::Ctx) -> ctx::Result<Box<DynClient<L2>>> {
        let addr = sync::wait_for(ctx, &mut self.addr.clone(), Option::is_some)
            .await?
            .unwrap();
        let client = Client::http(format!("http://{addr}/").parse().context("url")?)
            .context("json_rpc()")?
            .build();
        let client: Box<DynClient<L2>> = Box::new(client);
        // Wait until the server is actually available.
        loop {
            let res = ctx.wait(client.fetch_l2_block_number()).await?;
            match res {
                Ok(_) => return Ok(client),
                Err(err) if err.is_transient() => {
                    ctx.sleep(time::Duration::seconds(5)).await?;
                }
                Err(err) => {
                    return Err(anyhow::format_err!("{err}").into());
                }
            }
        }
    }

    /// Runs the centralized fetcher.
    pub async fn run_fetcher(
        self,
        ctx: &ctx::Ctx,
        client: Box<DynClient<L2>>,
    ) -> anyhow::Result<()> {
        en::EN {
            pool: self.pool,
            client,
            sync_state: self.sync_state.clone(),
        }
        .run_fetcher(ctx, self.actions_sender)
        .await
    }

    /// Runs consensus node for the external node.
    pub async fn run_consensus(
        self,
        ctx: &ctx::Ctx,
        client: Box<DynClient<L2>>,
        cfg: &network::Config,
    ) -> anyhow::Result<()> {
        let (cfg, secrets) = config(cfg);
        en::EN {
            pool: self.pool,
            client,
            sync_state: self.sync_state.clone(),
        }
        .run(ctx, self.actions_sender, cfg, secrets)
        .await
    }
}

async fn mock_commitment_generator_step(ctx: &ctx::Ctx, pool: &ConnectionPool) -> ctx::Result<()> {
    let mut conn = pool.connection(ctx).await.wrap("connection()")?;
    let Some(first) = ctx
        .wait(
            conn.0
                .blocks_dal()
                .get_next_l1_batch_ready_for_commitment_generation(),
        )
        .await?
        .map_err(|e| e.generalize())?
    else {
        return Ok(());
    };
    let last = ctx
        .wait(
            conn.0
                .blocks_dal()
                .get_last_l1_batch_ready_for_commitment_generation(),
        )
        .await?
        .map_err(|e| e.generalize())?
        .context("batch disappeared")?;
    // Create artificial `L1BatchCommitmentArtifacts`.
    for i in (first.0..=last.0).map(L1BatchNumber) {
        let metadata = create_l1_batch_metadata(i.0);
        let artifacts = l1_batch_metadata_to_commitment_artifacts(&metadata);
        ctx.wait(
            conn.0
                .blocks_dal()
                .save_l1_batch_commitment_artifacts(i, &artifacts),
        )
        .await??;
    }
    Ok(())
}

async fn mock_metadata_calculator_step(ctx: &ctx::Ctx, pool: &ConnectionPool) -> ctx::Result<()> {
    let mut conn = pool.connection(ctx).await.wrap("connection()")?;
    let Some(last) = ctx
        .wait(conn.0.blocks_dal().get_sealed_l1_batch_number())
        .await?
        .map_err(DalError::generalize)?
    else {
        return Ok(());
    };
    let prev = ctx
        .wait(
            conn.0
                .blocks_dal()
                .get_last_l1_batch_number_with_tree_data(),
        )
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
    // Executes the state keeper task with real metadata calculator task
    // and fake commitment generator (because real one is too slow).
    pub async fn run_real(self, ctx: &ctx::Ctx) -> anyhow::Result<()> {
        let res = scope::run!(ctx, |ctx, s| async {
            // Fund the test account. Required for L2 transactions to succeed.
            fund(&self.pool.0, &[self.account.address]).await;

            let (stop_send, stop_recv) = sync::watch::channel(false);
            let (persistence, l2_block_sealer) =
                StateKeeperPersistence::new(self.pool.0.clone(), Address::repeat_byte(11), 5);

            let io = ExternalIO::new(
                self.pool.0.clone(),
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

            s.spawn_bg({
                let stop_recv = stop_recv.clone();
                async {
                    self.metadata_calculator.run(stop_recv).await?;
                    Ok(())
                }
            });

            // TODO: should be replaceable with `PostgresFactory`.
            // Caching shouldn't be needed for tests.
            let (async_cache, async_catchup_task) = AsyncRocksdbCache::new(
                self.pool.0.clone(),
                self.rocksdb_dir
                    .path()
                    .join("cache")
                    .to_string_lossy()
                    .into(),
                RocksdbStorageOptions {
                    block_cache_capacity: (1 << 20), // `1MB`
                    max_open_files: None,
                },
            );
            s.spawn_bg({
                let stop_recv = stop_recv.clone();
                async {
                    async_catchup_task.run(stop_recv).await?;
                    Ok(())
                }
            });
            s.spawn_bg::<()>(async {
                loop {
                    mock_commitment_generator_step(ctx, &self.pool).await?;
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
                        Box::new(MainBatchExecutor::new(false, false)),
                        OutputHandler::new(Box::new(persistence.with_tx_insertion()))
                            .with_handler(Box::new(self.sync_state.clone())),
                        Arc::new(NoopSealer),
                        Arc::new(async_cache),
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
                    self.pool.0.clone(),
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

    /// Executes the StateKeeper task.
    pub async fn run(self, ctx: &ctx::Ctx) -> anyhow::Result<()> {
        let res = scope::run!(ctx, |ctx, s| async {
            let (stop_send, stop_recv) = sync::watch::channel(false);
            let (persistence, l2_block_sealer) =
                StateKeeperPersistence::new(self.pool.0.clone(), Address::repeat_byte(11), 5);
            let tree_writes_persistence = TreeWritesPersistence::new(self.pool.0.clone());

            let io = ExternalIO::new(
                self.pool.0.clone(),
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
                    mock_metadata_calculator_step(ctx, &self.pool).await?;
                    mock_commitment_generator_step(ctx, &self.pool).await?;
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
                        OutputHandler::new(Box::new(persistence.with_tx_insertion()))
                            .with_handler(Box::new(tree_writes_persistence))
                            .with_handler(Box::new(self.sync_state.clone())),
                        Arc::new(NoopSealer),
                        Arc::new(MockReadStorageFactory),
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
                    self.pool.0.clone(),
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

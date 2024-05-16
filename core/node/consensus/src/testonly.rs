//! Utilities for testing the consensus module.

use std::sync::Arc;

use anyhow::Context as _;
use rand::Rng;
use zksync_concurrency::{ctx, error::Wrap as _, scope, sync, time};
use zksync_config::{configs, configs::consensus as config};
use zksync_consensus_crypto::TextFmt as _;
use zksync_consensus_network as network;
use zksync_consensus_roles::validator;
use zksync_dal::{CoreDal, DalError};
use zksync_node_api_server::web3::{state::InternalApiConfig, testonly::spawn_http_server};
use zksync_node_genesis::GenesisParams;
use zksync_node_sync::{
    fetcher::{FetchedTransaction, IoCursorExt as _},
    sync_action::{ActionQueue, ActionQueueSender, SyncAction},
    testonly::MockMainNodeClient,
    ExternalIO, MainNodeClient, SyncState,
};
use zksync_node_test_utils::{create_l1_batch_metadata, create_l2_transaction};
use zksync_state_keeper::{
    io::{IoCursor, L1BatchParams, L2BlockParams},
    seal_criteria::NoopSealer,
    testonly::MockBatchExecutor,
    OutputHandler, StateKeeperPersistence, ZkSyncStateKeeper,
};
use zksync_types::{Address, L1BatchNumber, L2BlockNumber, L2ChainId, ProtocolVersionId};
use zksync_web3_decl::client::{Client, DynClient, L2};

use crate::{en, ConnectionPool};

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
    sync_state: SyncState,
    addr: sync::watch::Receiver<Option<std::net::SocketAddr>>,
    pool: ConnectionPool,
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
}

impl StateKeeper {
    /// Constructs and initializes a new `StateKeeper`.
    /// Caller has to run `StateKeeperRunner.run()` task in the background.
    pub async fn new(
        ctx: &ctx::Ctx,
        pool: ConnectionPool,
    ) -> ctx::Result<(Self, StateKeeperRunner)> {
        let mut conn = pool.connection(ctx).await.wrap("connection()")?;
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
        Ok((
            Self {
                last_batch: cursor.l1_batch,
                last_block: cursor.next_l2_block - 1,
                last_timestamp: cursor.prev_l2_block_timestamp,
                batch_sealed: !pending_batch,
                fee_per_gas: 10,
                gas_per_pubdata: 100,
                actions_sender,
                sync_state: sync_state.clone(),
                addr: addr.subscribe(),
                pool: pool.clone(),
            },
            StateKeeperRunner {
                actions_queue,
                sync_state,
                pool: pool.clone(),
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

async fn calculate_mock_metadata(ctx: &ctx::Ctx, pool: &ConnectionPool) -> ctx::Result<()> {
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
    /// Executes the StateKeeper task.
    pub async fn run(self, ctx: &ctx::Ctx) -> anyhow::Result<()> {
        let res = scope::run!(ctx, |ctx, s| async {
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
            s.spawn_bg::<()>(async {
                loop {
                    calculate_mock_metadata(ctx, &self.pool).await?;
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
                            .with_handler(Box::new(self.sync_state.clone())),
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

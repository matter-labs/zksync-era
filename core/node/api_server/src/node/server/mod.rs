use std::{collections::HashSet, num::NonZeroU32, sync::Arc, time::Duration};

use zksync_config::configs::api::{MaxResponseSize, Namespace};
use zksync_dal::node::{PoolResource, ReplicaPool};
use zksync_health_check::AppHealthCheck;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_shared_resources::{
    api::{BridgeAddressesHandle, SyncState},
    contracts::{L1ChainContractsResource, L1EcosystemContractsResource, L2ContractsResource},
    tree::TreeApiClient,
};
use zksync_web3_decl::{
    client::{DynClient, L2},
    node::SettlementModeResource,
};

use self::sealed_l2_block::SealedL2BlockUpdaterTask;
use crate::{
    tx_sender::TxSender,
    web3::{
        mempool_cache::MempoolCache,
        metrics::SubscriptionType,
        pubsub::{EthSubscribe, PubSubNotifier},
        state::{InternalApiConfig, InternalApiConfigBase, SealedL2BlockNumber},
        ApiBuilder, ApiServer, ApiTransport,
    },
};

mod sealed_l2_block;

/// Set of optional variables that can be altered to modify the behavior of API builder.
#[derive(Debug)]
pub struct Web3ServerOptionalConfig {
    pub http_namespaces: HashSet<Namespace>,
    pub ws_namespaces: HashSet<Namespace>,
    pub filters_limit: usize,
    pub subscriptions_limit: usize,
    pub batch_request_size_limit: usize,
    pub response_body_size_limit: MaxResponseSize,
    pub websocket_requests_per_minute_limit: Option<NonZeroU32>,
    pub request_timeout: Option<Duration>,
    pub with_extended_tracing: bool,
    pub polling_interval: Duration,
    // Used by the external node.
    pub pruning_info_refresh_interval: Duration,
}

impl Web3ServerOptionalConfig {
    fn apply(self, mut api_builder: ApiBuilder) -> ApiBuilder {
        api_builder = api_builder
            .enable_http_namespaces(self.http_namespaces)
            .enable_ws_namespaces(self.ws_namespaces)
            .with_filter_limit(self.filters_limit)
            .with_subscriptions_limit(self.subscriptions_limit)
            .with_batch_request_size_limit(self.batch_request_size_limit)
            .with_response_body_size_limit(self.response_body_size_limit)
            .with_extended_tracing(self.with_extended_tracing)
            .with_pruning_info_refresh_interval(self.pruning_info_refresh_interval);

        if let Some(websocket_requests_per_minute_limit) = self.websocket_requests_per_minute_limit
        {
            api_builder = api_builder
                .with_websocket_requests_per_minute_limit(websocket_requests_per_minute_limit);
        }
        if let Some(request_timeout) = self.request_timeout {
            api_builder = api_builder.with_request_timeout(request_timeout);
        }
        api_builder
    }
}

/// Wiring layer for Web3 JSON RPC server.
///
/// ## Requests resources
///
/// - `PoolResource<ReplicaPool>`
/// - `TxSenderResource`
/// - `SyncState` (optional)
/// - `TreeApiClientResource` (optional)
/// - `MempoolCacheResource`
/// - `CircuitBreakersResource` (adds a circuit breaker)
/// - `AppHealthCheckResource` (adds a health check)
///
/// ## Adds tasks
///
/// - `Web3ApiTask` -- wrapper for all the tasks spawned by the API.
#[derive(Debug)]
pub struct Web3ServerLayer {
    http_port: Option<u16>,
    ws_port: Option<u16>,
    optional_config: Web3ServerOptionalConfig,
    internal_api_config_base: InternalApiConfigBase,
}

#[derive(Debug, FromContext)]
pub struct Input {
    #[context(default)]
    bridge_addresses: BridgeAddressesHandle,
    replica_pool: PoolResource<ReplicaPool>,
    tx_sender: TxSender,
    sync_state: Option<SyncState>,
    tree_api_client: Option<Arc<dyn TreeApiClient>>,
    mempool_cache: MempoolCache,
    #[context(default)]
    app_health: Arc<AppHealthCheck>,
    main_node_client: Option<Box<DynClient<L2>>>,
    l1_contracts: L1ChainContractsResource,
    l1_ecosystem_contracts: L1EcosystemContractsResource,
    l2_contracts: L2ContractsResource,
    initial_settlement_mode: SettlementModeResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    http_server_task: Option<Web3ApiTask>,
    #[context(task)]
    ws_server_task: Option<Web3ApiTask>,
    #[context(task)]
    pub_sub_blocks_task: Option<PubSubNotifier>,
    #[context(task)]
    pub_sub_transactions_task: Option<PubSubNotifier>,
    #[context(task)]
    pub_sub_logs_task: Option<PubSubNotifier>,
    #[context(task)]
    sealed_l2_block_updater_task: SealedL2BlockUpdaterTask,
}

impl Web3ServerLayer {
    /// `None` ports means to not start the corresponding server.
    pub fn new(
        http_port: Option<u16>,
        ws_port: Option<u16>,
        internal_api_config_base: InternalApiConfigBase,
        optional_config: Web3ServerOptionalConfig,
    ) -> Self {
        assert!(
            http_port.is_some() || ws_port.is_some(),
            "useless `Web3ServerLayer` instantiation"
        );
        Self {
            http_port,
            ws_port,
            optional_config,
            internal_api_config_base,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for Web3ServerLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "web3_server_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        // Get required resources.
        let replica_resource_pool = input.replica_pool;
        let updaters_pool = replica_resource_pool.get_custom(1).await?;
        let replica_pool = replica_resource_pool.get().await?;
        let tx_sender = input.tx_sender;
        let mempool_cache = input.mempool_cache;
        let sync_state = input.sync_state;
        let tree_api_client = input.tree_api_client;

        let l1_contracts = input.l1_contracts.0;
        let internal_api_config = InternalApiConfig::from_base_and_contracts(
            self.internal_api_config_base,
            &l1_contracts,
            &input.l1_ecosystem_contracts.0,
            &input.l2_contracts.0,
            input
                .initial_settlement_mode
                .settlement_layer_for_sending_txs(),
        );
        let sealed_l2_block_handle = SealedL2BlockNumber::default();
        let bridge_addresses = input.bridge_addresses;
        bridge_addresses
            .update(internal_api_config.bridge_addresses.clone())
            .await;
        let sealed_l2_block_updater_task = SealedL2BlockUpdaterTask {
            number_updater: sealed_l2_block_handle.clone(),
            pool: updaters_pool,
        };

        // Build pub-sub notifier tasks.
        // We ignore `http_namespaces` because HTTP doesn't support pub-sub anyway.
        let contains_pub_sub_namespace = self
            .optional_config
            .ws_namespaces
            .contains(&Namespace::Pubsub);
        let enable_pub_sub = self.ws_port.is_some() && contains_pub_sub_namespace;
        let polling_interval = self.optional_config.polling_interval;
        let pub_sub = enable_pub_sub.then(|| EthSubscribe::new(polling_interval));
        let pub_sub_blocks_task = pub_sub
            .as_ref()
            .map(|pub_sub| pub_sub.create_notifier(SubscriptionType::Blocks, replica_pool.clone()));
        let pub_sub_transactions_task = pub_sub
            .as_ref()
            .map(|pub_sub| pub_sub.create_notifier(SubscriptionType::Txs, replica_pool.clone()));
        let pub_sub_logs_task = pub_sub
            .as_ref()
            .map(|pub_sub| pub_sub.create_notifier(SubscriptionType::Logs, replica_pool.clone()));

        // Build server.
        let mut api_builder = ApiBuilder::new(internal_api_config, replica_pool.clone())
            .with_tx_sender(tx_sender)
            .with_mempool_cache(mempool_cache)
            .with_sealed_l2_block_handle(sealed_l2_block_handle)
            .with_bridge_addresses_handle(bridge_addresses);
        if let Some(client) = tree_api_client {
            api_builder = api_builder.with_tree_api(client);
        }
        if let Some(sync_state) = sync_state {
            api_builder = api_builder.with_sync_state(sync_state);
        }
        if let Some(main_node_client) = input.main_node_client {
            api_builder = api_builder.with_l2_l1_log_proof_handler(main_node_client);
        }
        api_builder = self.optional_config.apply(api_builder);

        let mut http_server = None;
        let mut ws_server = None;
        match (self.http_port, self.ws_port) {
            (Some(http_port), Some(ws_port)) if http_port == ws_port => {
                http_server = Some(api_builder.http_and_ws(http_port).build()?);
            }
            (Some(http_port), Some(ws_port)) => {
                // Two distinct ports.
                http_server = Some(api_builder.clone().http(http_port).build()?);
                ws_server = Some(api_builder.ws(ws_port).build()?);
            }
            (Some(port), None) => {
                http_server = Some(api_builder.http(port).build()?);
            }
            (None, Some(port)) => {
                ws_server = Some(api_builder.ws(port).build()?);
            }
            (None, None) => unreachable!("validated in constructor"),
        }

        // Insert healthchecks.
        if let Some(server) = &http_server {
            for health_check in server.health_checks() {
                input
                    .app_health
                    .insert_component(health_check)
                    .map_err(WiringError::internal)?;
            }
        }
        let http_server_task = http_server.map(|server| Web3ApiTask {
            server,
            pub_sub: pub_sub.clone(),
        });

        if let Some(server) = &ws_server {
            for health_check in server.health_checks() {
                input
                    .app_health
                    .insert_component(health_check)
                    .map_err(WiringError::internal)?;
            }
        }
        let ws_server_task = ws_server.map(|server| Web3ApiTask { server, pub_sub });

        Ok(Output {
            http_server_task,
            ws_server_task,
            pub_sub_blocks_task,
            pub_sub_transactions_task,
            pub_sub_logs_task,
            sealed_l2_block_updater_task,
        })
    }
}

#[async_trait::async_trait]
impl Task for PubSubNotifier {
    fn id(&self) -> TaskId {
        match self.subscription_type() {
            SubscriptionType::Blocks => "api/pub_sub_notifiers/blocks".into(),
            SubscriptionType::Txs => "api/pub_sub_notifiers/txs".into(),
            SubscriptionType::Logs => "api/pub_sub_notifiers/logs".into(),
        }
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

/// Wrapper for the Web3 API.
#[derive(Debug)]
pub struct Web3ApiTask {
    server: ApiServer,
    pub_sub: Option<EthSubscribe>,
}

#[async_trait::async_trait]
impl Task for Web3ApiTask {
    fn id(&self) -> TaskId {
        match self.server.transport() {
            ApiTransport::Http(_) => "web3_http_server".into(),
            ApiTransport::Ws(_) => "web3_ws_server".into(),
            ApiTransport::HttpAndWs(_) => "web3_http_and_ws_server".into(),
        }
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.server.run(self.pub_sub, stop_receiver.0).await
    }
}

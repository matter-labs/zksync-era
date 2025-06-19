use std::{num::NonZeroU32, sync::Arc, time::Duration};

use zksync_circuit_breaker::{replication_lag::ReplicationLagChecker, CircuitBreakers};
use zksync_config::configs::api::MaxResponseSize;
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
        ApiBuilder, ApiServer, Namespace,
    },
};

mod sealed_l2_block;

/// Set of optional variables that can be altered to modify the behavior of API builder.
#[derive(Debug, Default)]
pub struct Web3ServerOptionalConfig {
    pub namespaces: Option<Vec<Namespace>>,
    pub filters_limit: Option<usize>,
    pub subscriptions_limit: Option<usize>,
    pub batch_request_size_limit: Option<usize>,
    pub response_body_size_limit: Option<MaxResponseSize>,
    pub websocket_requests_per_minute_limit: Option<NonZeroU32>,
    pub request_timeout: Option<Duration>,
    pub with_extended_tracing: bool,
    // Used by circuit breaker.
    pub replication_lag_limit: Option<Duration>,
    // Used by the external node.
    pub pruning_info_refresh_interval: Option<Duration>,
    // Used by the external node.
    pub polling_interval: Option<Duration>,
}

impl Web3ServerOptionalConfig {
    fn apply(self, mut api_builder: ApiBuilder) -> ApiBuilder {
        if let Some(namespaces) = self.namespaces {
            api_builder = api_builder.enable_api_namespaces(namespaces);
        }
        if let Some(filters_limit) = self.filters_limit {
            api_builder = api_builder.with_filter_limit(filters_limit);
        }
        if let Some(subscriptions_limit) = self.subscriptions_limit {
            api_builder = api_builder.with_subscriptions_limit(subscriptions_limit);
        }
        if let Some(batch_request_size_limit) = self.batch_request_size_limit {
            api_builder = api_builder.with_batch_request_size_limit(batch_request_size_limit);
        }
        if let Some(response_body_size_limit) = self.response_body_size_limit {
            api_builder = api_builder.with_response_body_size_limit(response_body_size_limit);
        }
        if let Some(websocket_requests_per_minute_limit) = self.websocket_requests_per_minute_limit
        {
            api_builder = api_builder
                .with_websocket_requests_per_minute_limit(websocket_requests_per_minute_limit);
        }
        if let Some(request_timeout) = self.request_timeout {
            api_builder = api_builder.with_request_timeout(request_timeout);
        }
        if let Some(polling_interval) = self.polling_interval {
            api_builder = api_builder.with_polling_interval(polling_interval);
        }
        if let Some(pruning_info_refresh_interval) = self.pruning_info_refresh_interval {
            api_builder =
                api_builder.with_pruning_info_refresh_interval(pruning_info_refresh_interval);
        }
        api_builder = api_builder.with_extended_tracing(self.with_extended_tracing);
        api_builder
    }
}

/// Internal-only marker of chosen transport.
#[derive(Debug, Clone, Copy)]
enum Transport {
    Http,
    Ws,
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
    transport: Transport,
    port: u16,
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
    circuit_breakers: Arc<CircuitBreakers>,
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
    web3_api_task: Web3ApiTask,
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
    pub fn http(
        port: u16,
        internal_api_config_base: InternalApiConfigBase,
        optional_config: Web3ServerOptionalConfig,
    ) -> Self {
        Self {
            transport: Transport::Http,
            port,
            optional_config,
            internal_api_config_base,
        }
    }

    pub fn ws(
        port: u16,
        internal_api_config_base: InternalApiConfigBase,
        optional_config: Web3ServerOptionalConfig,
    ) -> Self {
        Self {
            transport: Transport::Ws,
            port,
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
        match self.transport {
            Transport::Http => "web3_http_server_layer",
            Transport::Ws => "web3_ws_server_layer",
        }
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
        let contains_pub_sub_namespace = self
            .optional_config
            .namespaces
            .as_ref()
            .is_none_or(|namespaces| namespaces.contains(&Namespace::Pubsub));
        let polling_interval = self
            .optional_config
            .polling_interval
            .unwrap_or(Duration::from_millis(100)); // FIXME: don't use hard-coded default
        let enable_pub_sub = matches!(self.transport, Transport::Ws) && contains_pub_sub_namespace;
        let pub_sub = enable_pub_sub.then(EthSubscribe::new);
        let pub_sub_blocks_task = pub_sub.as_ref().map(|pub_sub| {
            pub_sub.create_notifier(
                SubscriptionType::Blocks,
                replica_pool.clone(),
                polling_interval,
            )
        });
        let pub_sub_transactions_task = pub_sub.as_ref().map(|pub_sub| {
            pub_sub.create_notifier(
                SubscriptionType::Txs,
                replica_pool.clone(),
                polling_interval,
            )
        });
        let pub_sub_logs_task = pub_sub.as_ref().map(|pub_sub| {
            pub_sub.create_notifier(
                SubscriptionType::Logs,
                replica_pool.clone(),
                polling_interval,
            )
        });

        // Build server.
        let mut api_builder =
            ApiBuilder::jsonrpsee_backend(internal_api_config, replica_pool.clone())
                .with_tx_sender(tx_sender)
                .with_mempool_cache(mempool_cache)
                .with_extended_tracing(self.optional_config.with_extended_tracing)
                .with_sealed_l2_block_handle(sealed_l2_block_handle)
                .with_bridge_addresses_handle(bridge_addresses);
        if let Some(client) = tree_api_client {
            api_builder = api_builder.with_tree_api(client);
        }
        match self.transport {
            Transport::Http => {
                api_builder = api_builder.http(self.port);
            }
            Transport::Ws => {
                api_builder = api_builder.ws(self.port);
            }
        }
        if let Some(sync_state) = sync_state {
            api_builder = api_builder.with_sync_state(sync_state);
        }
        if let Some(main_node_client) = input.main_node_client {
            api_builder = api_builder.with_l2_l1_log_proof_handler(main_node_client);
        }
        let replication_lag_limit = self.optional_config.replication_lag_limit;
        api_builder = self.optional_config.apply(api_builder);

        let server = api_builder.build()?;

        // Insert healthcheck.
        let api_health_check = server.health_check();
        input
            .app_health
            .insert_component(api_health_check)
            .map_err(WiringError::internal)?;

        // Insert circuit breaker.
        input
            .circuit_breakers
            .insert(Box::new(ReplicationLagChecker {
                pool: replica_pool,
                replication_lag_limit,
            }))
            .await;

        // Add tasks.
        let web3_api_task = Web3ApiTask {
            transport: self.transport,
            pub_sub,
            server,
        };
        Ok(Output {
            web3_api_task,
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
    transport: Transport,
    server: ApiServer,
    pub_sub: Option<EthSubscribe>,
}

#[async_trait::async_trait]
impl Task for Web3ApiTask {
    fn id(&self) -> TaskId {
        match self.transport {
            Transport::Http => "web3_http_server".into(),
            Transport::Ws => "web3_ws_server".into(),
        }
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.server.run(self.pub_sub, stop_receiver.0).await
    }
}

use std::{num::NonZeroU32, time::Duration};

use tokio::{sync::oneshot, task::JoinHandle};
use zksync_circuit_breaker::{di::CircuitBreakersResource, replication_lag::ReplicationLagChecker};
use zksync_config::configs::api::MaxResponseSize;
use zksync_dal::di::{PoolResource, ReplicaPool};
use zksync_health_check::di::AppHealthCheckResource;
use zksync_metadata_calculator::di::TreeApiClientResource;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_shared_di::{
    api::{BridgeAddressesHandle, SyncState},
    contracts::{
        L1ChainContractsResource, L1EcosystemContractsResource, L2ContractsResource,
        SettlementLayerContractsResource,
    },
};
use zksync_web3_decl::di::{EthInterfaceResource, MainNodeClientResource, SettlementModeResource};

use self::sealed_l2_block::SealedL2BlockUpdaterTask;
use super::resources::{MempoolCacheResource, TxSenderResource};
use crate::web3::{
    state::{InternalApiConfig, InternalApiConfigBase, SealedL2BlockNumber},
    ApiBuilder, ApiServer, Namespace,
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
/// - `ApiTaskGarbageCollector` -- maintenance task that manages API tasks.
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
    pub bridge_addresses: BridgeAddressesHandle,
    pub replica_pool: PoolResource<ReplicaPool>,
    pub tx_sender: TxSenderResource,
    pub sync_state: Option<SyncState>,
    pub tree_api_client: Option<TreeApiClientResource>,
    pub mempool_cache: MempoolCacheResource,
    #[context(default)]
    pub circuit_breakers: CircuitBreakersResource,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
    pub main_node_client: Option<MainNodeClientResource>,
    pub l1_client: EthInterfaceResource,
    pub sl_contracts: SettlementLayerContractsResource,
    pub l1_contracts: L1ChainContractsResource,
    pub l1_ecosystem_contracts: L1EcosystemContractsResource,
    pub l2_contracts: L2ContractsResource,
    pub initial_settlement_mode: SettlementModeResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub web3_api_task: Web3ApiTask,
    #[context(task)]
    pub garbage_collector_task: ApiTaskGarbageCollector,
    #[context(task)]
    pub sealed_l2_block_updater_task: SealedL2BlockUpdaterTask,
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
        let TxSenderResource(tx_sender) = input.tx_sender;
        let MempoolCacheResource(mempool_cache) = input.mempool_cache;
        let sync_state = input.sync_state;
        let tree_api_client = input.tree_api_client.map(|client| client.0);

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
            api_builder = api_builder.with_l2_l1_log_proof_handler(main_node_client.0)
        }
        let replication_lag_limit = self.optional_config.replication_lag_limit;
        api_builder = self.optional_config.apply(api_builder);

        let server = api_builder.build()?;

        // Insert healthcheck.
        let api_health_check = server.health_check();
        input
            .app_health
            .0
            .insert_component(api_health_check)
            .map_err(WiringError::internal)?;

        // Insert circuit breaker.
        input
            .circuit_breakers
            .breakers
            .insert(Box::new(ReplicationLagChecker {
                pool: replica_pool,
                replication_lag_limit,
            }))
            .await;

        // Add tasks.
        let (task_sender, task_receiver) = oneshot::channel();
        let web3_api_task = Web3ApiTask {
            transport: self.transport,
            server,
            task_sender,
        };
        let garbage_collector_task = ApiTaskGarbageCollector { task_receiver };
        Ok(Output {
            web3_api_task,
            garbage_collector_task,
            sealed_l2_block_updater_task,
        })
    }
}

/// Wrapper for the Web3 API.
///
/// Internal design note: API infrastructure was already established and consists of a dynamic set of tasks,
/// and it proven to work well enough. It doesn't seem to be reasonable to refactor it to expose raw futures instead
/// of tokio tasks, since it'll require a lot of effort. So instead, we spawn all the tasks in this wrapper,
/// wait for the first one to finish, and then send the rest of the tasks to a special "garbage collector" task
/// which will wait for remaining tasks to finish.
/// All of this relies on the fact that the existing internal API tasks are aware of stop receiver: when we'll exit
/// this task on first API task completion, the rest of the tasks will be stopped as well.
// TODO (QIT-26): Once we switch the codebase to only use the framework, we need to properly refactor the API to only
// use abstractions provided by this framework and not spawn any tasks on its own.
#[derive(Debug)]
pub struct Web3ApiTask {
    transport: Transport,
    server: ApiServer,
    task_sender: oneshot::Sender<Vec<ApiJoinHandle>>,
}

type ApiJoinHandle = JoinHandle<anyhow::Result<()>>;

#[async_trait::async_trait]
impl Task for Web3ApiTask {
    fn id(&self) -> TaskId {
        match self.transport {
            Transport::Http => "web3_http_server".into(),
            Transport::Ws => "web3_ws_server".into(),
        }
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let tasks = self.server.run(stop_receiver.0).await?;
        // Wait for the first task to finish to be able to signal the service.
        let (result, _idx, rem) = futures::future::select_all(tasks.tasks).await;
        // Send remaining tasks to the garbage collector.
        let _ = self.task_sender.send(rem);
        result?
    }
}

/// Helper task that waits for a list of task join handles and then awaits them all.
/// For more details, see [`Web3ApiTask`].
#[derive(Debug)]
pub struct ApiTaskGarbageCollector {
    task_receiver: oneshot::Receiver<Vec<ApiJoinHandle>>,
}

#[async_trait::async_trait]
impl Task for ApiTaskGarbageCollector {
    fn id(&self) -> TaskId {
        "api_task_garbage_collector".into()
    }

    async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
        // We can ignore a stop request here, since we're tied to the main API task through the channel:
        // it'll either get dropped if API cannot be built or will send something through the channel.
        // The tasks it sends are aware of the stop receiver themselves.
        let Ok(tasks) = self.task_receiver.await else {
            // API cannot be built, so there are no tasks to wait for.
            return Ok(());
        };
        let _ = futures::future::join_all(tasks).await;
        Ok(())
    }
}

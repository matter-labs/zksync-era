use std::num::NonZeroU32;

use tokio::{sync::oneshot, task::JoinHandle};
use zksync_core::api_server::web3::{state::InternalApiConfig, ApiBuilder, ApiServer, Namespace};

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource, pools::ReplicaPoolResource,
        sync_state::SyncStateResource, web3_api::TxSenderResource,
    },
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

/// Set of optional variables that can be altered to modify the behavior of API builder.
#[derive(Debug, Default)]
pub struct Web3ServerOptionalConfig {
    pub namespaces: Option<Vec<Namespace>>,
    pub filters_limit: Option<usize>,
    pub subscriptions_limit: Option<usize>,
    pub batch_request_size_limit: Option<usize>,
    pub response_body_size_limit: Option<usize>,
    pub websocket_requests_per_minute_limit: Option<NonZeroU32>,
    pub tree_api_url: Option<String>,
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
        api_builder = api_builder.with_tree_api(self.tree_api_url);
        api_builder
    }
}

/// Internal-only marker of chosen transport.
#[derive(Debug, Clone, Copy)]
enum Transport {
    Http,
    Ws,
}

#[derive(Debug)]
pub struct Web3ServerLayer {
    transport: Transport,
    port: u16,
    internal_api_config: InternalApiConfig,
    optional_config: Web3ServerOptionalConfig,
}

impl Web3ServerLayer {
    pub fn http(
        port: u16,
        internal_api_config: InternalApiConfig,
        optional_config: Web3ServerOptionalConfig,
    ) -> Self {
        Self {
            transport: Transport::Http,
            port,
            internal_api_config,
            optional_config,
        }
    }

    pub fn ws(
        port: u16,
        internal_api_config: InternalApiConfig,
        optional_config: Web3ServerOptionalConfig,
    ) -> Self {
        Self {
            transport: Transport::Ws,
            port,
            internal_api_config,
            optional_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for Web3ServerLayer {
    fn layer_name(&self) -> &'static str {
        match self.transport {
            Transport::Http => "web3_http_server_layer",
            Transport::Ws => "web3_ws_server_layer",
        }
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        // Get required resources.
        let replica_resource_pool = context.get_resource::<ReplicaPoolResource>().await?;
        let updaters_pool = replica_resource_pool.get_custom(2).await?;
        let replica_pool = replica_resource_pool.get().await?;
        let tx_sender = context.get_resource::<TxSenderResource>().await?.0;
        let sync_state = match context.get_resource::<SyncStateResource>().await {
            Ok(sync_state) => Some(sync_state.0),
            Err(WiringError::ResourceLacking(_)) => None,
            Err(err) => {
                return Err(err);
            }
        };

        // Build server.
        let mut api_builder = ApiBuilder::jsonrpsee_backend(self.internal_api_config, replica_pool)
            .with_updaters_pool(updaters_pool)
            .with_tx_sender(tx_sender);
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
        api_builder = self.optional_config.apply(api_builder);
        let server = api_builder.build()?;

        // Insert healthcheck.
        let api_health_check = server.health_check();
        let AppHealthCheckResource(app_health) = context.get_resource_or_default().await;
        app_health.insert_component(api_health_check);

        // Add tasks.
        let (task_sender, task_receiver) = oneshot::channel();
        let web3_api_task = Web3ApiTask {
            transport: self.transport,
            server,
            task_sender,
        };
        let garbage_collector_task = ApiTaskGarbageCollector { task_receiver };
        context.add_task(Box::new(web3_api_task));
        context.add_task(Box::new(garbage_collector_task));

        Ok(())
    }
}

/// Wrapper for the Web3 API.
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
struct Web3ApiTask {
    transport: Transport,
    server: ApiServer,
    task_sender: oneshot::Sender<Vec<ApiJoinHandle>>,
}

type ApiJoinHandle = JoinHandle<anyhow::Result<()>>;

#[async_trait::async_trait]
impl Task for Web3ApiTask {
    fn name(&self) -> &'static str {
        match self.transport {
            Transport::Http => "web3_http_server",
            Transport::Ws => "web3_ws_server",
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
struct ApiTaskGarbageCollector {
    task_receiver: oneshot::Receiver<Vec<ApiJoinHandle>>,
}

#[async_trait::async_trait]
impl Task for ApiTaskGarbageCollector {
    fn name(&self) -> &'static str {
        "api_task_garbage_collector"
    }

    async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
        // We can ignore the stop signal here, since we're tied to the main API task through the channel:
        // it'll either get dropped if API cannot be built or will send something through the channel.
        // The tasks it sends are aware of the stop receiver themselves.
        let tasks = self.task_receiver.await?;
        let _ = futures::future::join_all(tasks).await;
        Ok(())
    }
}

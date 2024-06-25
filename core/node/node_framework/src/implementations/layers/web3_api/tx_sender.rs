use std::{fmt, sync::Arc, time::Duration};

use tokio::sync::RwLock;
use zksync_node_api_server::{
    execution_sandbox::{VmConcurrencyBarrier, VmConcurrencyLimiter},
    tx_sender::{ApiContracts, TxSenderBuilder, TxSenderConfig},
};
use zksync_state::PostgresStorageCaches;
use zksync_types::Address;
use zksync_web3_decl::{
    client::{DynClient, L2},
    jsonrpsee,
    namespaces::EnNamespaceClient as _,
};

use crate::{
    implementations::resources::{
        fee_input::FeeInputResource,
        main_node_client::MainNodeClientResource,
        pools::{PoolResource, ReplicaPool},
        state_keeper::ConditionalSealerResource,
        web3_api::{TxSenderResource, TxSinkResource},
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct PostgresStorageCachesConfig {
    pub factory_deps_cache_size: u64,
    pub initial_writes_cache_size: u64,
    pub latest_values_cache_size: u64,
}

/// Wiring layer for the `TxSender`.
/// Prepares the `TxSender` itself, as well as the tasks required for its maintenance.
///
/// ## Requests resources
///
/// - `TxSinkResource`
/// - `PoolResource<ReplicaPool>`
/// - `ConditionalSealerResource` (optional)
/// - `FeeInputResource`
///
/// ## Adds resources
///
/// - `TxSenderResource`
///
/// ## Adds tasks
///
/// - `PostgresStorageCachesTask`
/// - `VmConcurrencyBarrierTask`
/// - `WhitelistedTokensForAaUpdateTask` (optional)
#[derive(Debug)]
pub struct TxSenderLayer {
    tx_sender_config: TxSenderConfig,
    postgres_storage_caches_config: PostgresStorageCachesConfig,
    max_vm_concurrency: usize,
    api_contracts: ApiContracts,
    whitelisted_tokens_for_aa_cache: bool,
}

impl TxSenderLayer {
    pub fn new(
        tx_sender_config: TxSenderConfig,
        postgres_storage_caches_config: PostgresStorageCachesConfig,
        max_vm_concurrency: usize,
        api_contracts: ApiContracts,
    ) -> Self {
        Self {
            tx_sender_config,
            postgres_storage_caches_config,
            max_vm_concurrency,
            api_contracts,
            whitelisted_tokens_for_aa_cache: false,
        }
    }

    /// Enables the task for fetching the whitelisted tokens for the AA cache from the main node.
    /// Disabled by default.
    ///
    /// Requires `MainNodeClientResource` to be present.
    pub fn with_whitelisted_tokens_for_aa_cache(mut self, value: bool) -> Self {
        self.whitelisted_tokens_for_aa_cache = value;
        self
    }
}

#[async_trait::async_trait]
impl WiringLayer for TxSenderLayer {
    fn layer_name(&self) -> &'static str {
        "tx_sender_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        // Get required resources.
        let tx_sink = context.get_resource::<TxSinkResource>().await?.0;
        let pool_resource = context.get_resource::<PoolResource<ReplicaPool>>().await?;
        let replica_pool = pool_resource.get().await?;
        let sealer = match context.get_resource::<ConditionalSealerResource>().await {
            Ok(sealer) => Some(sealer.0),
            Err(WiringError::ResourceLacking { .. }) => None,
            Err(other) => return Err(other),
        };
        let fee_input = context.get_resource::<FeeInputResource>().await?.0;

        // Initialize Postgres caches.
        let factory_deps_capacity = self.postgres_storage_caches_config.factory_deps_cache_size;
        let initial_writes_capacity = self
            .postgres_storage_caches_config
            .initial_writes_cache_size;
        let values_capacity = self.postgres_storage_caches_config.latest_values_cache_size;
        let mut storage_caches =
            PostgresStorageCaches::new(factory_deps_capacity, initial_writes_capacity);

        if values_capacity > 0 {
            let values_cache_task = storage_caches
                .configure_storage_values_cache(values_capacity, replica_pool.clone());
            context.add_task(Box::new(PostgresStorageCachesTask {
                task: values_cache_task,
            }));
        }

        // Initialize `VmConcurrencyLimiter`.
        let (vm_concurrency_limiter, vm_concurrency_barrier) =
            VmConcurrencyLimiter::new(self.max_vm_concurrency);
        context.add_task(Box::new(VmConcurrencyBarrierTask {
            barrier: vm_concurrency_barrier,
        }));

        // Build `TxSender`.
        let mut tx_sender = TxSenderBuilder::new(self.tx_sender_config, replica_pool, tx_sink);
        if let Some(sealer) = sealer {
            tx_sender = tx_sender.with_sealer(sealer);
        }

        // Add the task for updating the whitelisted tokens for the AA cache.
        if self.whitelisted_tokens_for_aa_cache {
            let MainNodeClientResource(main_node_client) = context.get_resource().await?;
            let whitelisted_tokens = Arc::new(RwLock::new(Default::default()));
            context.add_task(Box::new(WhitelistedTokensForAaUpdateTask {
                whitelisted_tokens: whitelisted_tokens.clone(),
                main_node_client,
            }));
            tx_sender = tx_sender.with_whitelisted_tokens_for_aa(whitelisted_tokens);
        }

        let tx_sender = tx_sender.build(
            fee_input,
            Arc::new(vm_concurrency_limiter),
            self.api_contracts,
            storage_caches,
        );
        context.insert_resource(TxSenderResource(tx_sender))?;

        Ok(())
    }
}

struct PostgresStorageCachesTask {
    task: zksync_state::PostgresStorageCachesTask,
}

impl fmt::Debug for PostgresStorageCachesTask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PostgresStorageCachesTask")
            .finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl Task for PostgresStorageCachesTask {
    fn id(&self) -> TaskId {
        "postgres_storage_caches".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.task.run(stop_receiver.0).await
    }
}

struct VmConcurrencyBarrierTask {
    barrier: VmConcurrencyBarrier,
}

#[async_trait::async_trait]
impl Task for VmConcurrencyBarrierTask {
    fn id(&self) -> TaskId {
        "vm_concurrency_barrier_task".into()
    }

    async fn run(mut self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        // Wait for the stop signal.
        stop_receiver.0.changed().await?;
        // Stop signal was received: seal the barrier so that no new VM requests are accepted.
        self.barrier.close();
        // Wait until all the existing API requests are processed.
        // We don't have to synchronize this with API servers being stopped, as they can decide themselves how to handle
        // ongoing requests during the shutdown.
        // We don't have to implement a timeout here either, as it'll be handled by the framework itself.
        self.barrier.wait_until_stopped().await;
        Ok(())
    }
}

#[derive(Debug)]
struct WhitelistedTokensForAaUpdateTask {
    whitelisted_tokens: Arc<RwLock<Vec<Address>>>,
    main_node_client: Box<DynClient<L2>>,
}

#[async_trait::async_trait]
impl Task for WhitelistedTokensForAaUpdateTask {
    fn id(&self) -> TaskId {
        "whitelisted_tokens_for_aa_update_task".into()
    }

    async fn run(mut self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        while !*stop_receiver.0.borrow_and_update() {
            match self.main_node_client.whitelisted_tokens_for_aa().await {
                Ok(tokens) => {
                    *self.whitelisted_tokens.write().await = tokens;
                }
                Err(jsonrpsee::core::client::Error::Call(error))
                    if error.code() == jsonrpsee::types::error::METHOD_NOT_FOUND_CODE =>
                {
                    // Method is not supported by the main node, do nothing.
                }
                Err(err) => {
                    tracing::error!("Failed to query `whitelisted_tokens_for_aa`, error: {err:?}");
                }
            }

            // Error here corresponds to a timeout w/o `stop_receiver` changed; we're OK with this.
            tokio::time::timeout(Duration::from_secs(30), stop_receiver.0.changed())
                .await
                .ok();
        }
        Ok(())
    }
}

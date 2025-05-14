use std::{sync::Arc, time::Duration};

use tokio::sync::RwLock;
use zksync_config::configs::chain::TimestampAsserterConfig;
use zksync_dal::node::{PoolResource, ReplicaPool};
use zksync_health_check::node::AppHealthCheckResource;
use zksync_node_fee_model::node::ApiFeeInputResource;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_object_store::node::ObjectStoreResource;
use zksync_shared_di::contracts::{L2ContractsResource, SettlementLayerContractsResource};
use zksync_state::{PostgresStorageCaches, PostgresStorageCachesTask};
use zksync_state_keeper::node::ConditionalSealerResource;
use zksync_types::{vm::FastVmMode, AccountTreeId, Address};
use zksync_web3_decl::{
    client::{DynClient, L2},
    jsonrpsee,
    namespaces::EnNamespaceClient as _,
    node::MainNodeClientResource,
};

use super::resources::{TxSenderResource, TxSinkResource};
use crate::{
    execution_sandbox::{VmConcurrencyBarrier, VmConcurrencyLimiter},
    tx_sender::{SandboxExecutorOptions, TimestampAsserterParams, TxSenderBuilder, TxSenderConfig},
};

#[derive(Debug)]
pub struct PostgresStorageCachesConfig {
    pub factory_deps_cache_size: u64,
    pub initial_writes_cache_size: u64,
    pub latest_values_cache_size: u64,
    pub latest_values_max_block_lag: u32,
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
    postgres_storage_caches_config: PostgresStorageCachesConfig,
    max_vm_concurrency: usize,
    whitelisted_tokens_for_aa_cache: bool,
    vm_mode: FastVmMode,
    timestamp_asserter_config: Option<TimestampAsserterConfig>,
    tx_sender_config: TxSenderConfig,
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub app_health: AppHealthCheckResource,
    pub tx_sink: TxSinkResource,
    pub replica_pool: PoolResource<ReplicaPool>,
    pub fee_input: ApiFeeInputResource,
    pub main_node_client: Option<MainNodeClientResource>,
    pub sealer: Option<ConditionalSealerResource>,
    pub sl_contracts: SettlementLayerContractsResource,
    pub l2_contracts: L2ContractsResource,
    pub core_object_store: Option<ObjectStoreResource>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    tx_sender: TxSenderResource,
    #[context(task)]
    vm_concurrency_barrier: VmConcurrencyBarrier,
    #[context(task)]
    postgres_storage_caches_task: Option<PostgresStorageCachesTaskWrapper>,
    #[context(task)]
    whitelisted_tokens_for_aa_update_task: Option<WhitelistedTokensForAaUpdateTask>,
}

impl TxSenderLayer {
    pub fn new(
        postgres_storage_caches_config: PostgresStorageCachesConfig,
        max_vm_concurrency: usize,
        tx_sender_config: TxSenderConfig,
        timestamp_asserter_config: Option<TimestampAsserterConfig>,
    ) -> Self {
        Self {
            postgres_storage_caches_config,
            max_vm_concurrency,
            whitelisted_tokens_for_aa_cache: false,
            vm_mode: FastVmMode::Old,
            timestamp_asserter_config,
            tx_sender_config,
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

    /// Sets the fast VM modes used for all supported operations.
    pub fn with_vm_mode(mut self, mode: FastVmMode) -> Self {
        self.vm_mode = mode;
        self
    }
}

#[async_trait::async_trait]
impl WiringLayer for TxSenderLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "tx_sender_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        // Get required resources.
        let tx_sink = input.tx_sink.0;
        let replica_pool = input.replica_pool.get().await?;
        let sealer = input.sealer.map(|s| s.0);
        let fee_input = input.fee_input.0;

        let config = match input.l2_contracts.0.timestamp_asserter_addr {
            Some(address) => {
                let timestamp_asserter_config =
                    self.timestamp_asserter_config.expect("Should be presented");

                self.tx_sender_config
                    .with_timestamp_asserter_params(TimestampAsserterParams {
                        address,
                        min_time_till_end: Duration::from_secs(
                            timestamp_asserter_config.min_time_till_end_sec.into(),
                        ),
                    })
            }
            None => self.tx_sender_config,
        };

        // Initialize Postgres caches.
        let factory_deps_capacity = self.postgres_storage_caches_config.factory_deps_cache_size;
        let initial_writes_capacity = self
            .postgres_storage_caches_config
            .initial_writes_cache_size;
        let values_capacity = self.postgres_storage_caches_config.latest_values_cache_size;
        let mut storage_caches =
            PostgresStorageCaches::new(factory_deps_capacity, initial_writes_capacity);

        let postgres_storage_caches_task = if values_capacity > 0 {
            let update_task = storage_caches.configure_storage_values_cache(
                values_capacity,
                self.postgres_storage_caches_config
                    .latest_values_max_block_lag,
                replica_pool.clone(),
            );
            Some(PostgresStorageCachesTaskWrapper(update_task))
        } else {
            None
        };

        // Initialize `VmConcurrencyLimiter`.
        let (vm_concurrency_limiter, vm_concurrency_barrier) =
            VmConcurrencyLimiter::new(self.max_vm_concurrency);

        // TODO (BFT-138): Allow to dynamically reload API contracts

        let mut executor_options = SandboxExecutorOptions::new(
            config.chain_id,
            AccountTreeId::new(config.fee_account_addr),
            config.validation_computational_gas_limit,
        )
        .await?;
        executor_options.set_fast_vm_mode(self.vm_mode);

        if let Some(store) = input.core_object_store {
            executor_options.set_vm_dump_object_store(store.0);
        }

        // Build `TxSender`.
        let mut tx_sender = TxSenderBuilder::new(config, replica_pool, tx_sink);
        if let Some(sealer) = sealer {
            tx_sender = tx_sender.with_sealer(sealer);
        }

        // Add the task for updating the whitelisted tokens for the AA cache.
        let whitelisted_tokens_for_aa_update_task = if self.whitelisted_tokens_for_aa_cache {
            let MainNodeClientResource(main_node_client) =
                input.main_node_client.ok_or_else(|| {
                    WiringError::Configuration(
                        "Main node client is required for the whitelisted tokens for AA cache"
                            .into(),
                    )
                })?;
            let whitelisted_tokens = Arc::new(RwLock::new(Default::default()));
            tx_sender = tx_sender.with_whitelisted_tokens_for_aa(whitelisted_tokens.clone());
            Some(WhitelistedTokensForAaUpdateTask {
                whitelisted_tokens: whitelisted_tokens.clone(),
                main_node_client,
            })
        } else {
            None
        };

        let tx_sender = tx_sender.build(
            fee_input,
            Arc::new(vm_concurrency_limiter),
            executor_options,
            storage_caches,
        );
        input
            .app_health
            .0
            .insert_custom_component(Arc::new(tx_sender.health_check()))
            .map_err(WiringError::internal)?;

        Ok(Output {
            tx_sender: tx_sender.into(),
            postgres_storage_caches_task,
            vm_concurrency_barrier,
            whitelisted_tokens_for_aa_update_task,
        })
    }
}

#[derive(Debug)]
struct PostgresStorageCachesTaskWrapper(PostgresStorageCachesTask);

#[async_trait::async_trait]
impl Task for PostgresStorageCachesTaskWrapper {
    fn id(&self) -> TaskId {
        "postgres_storage_caches".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.0.run(stop_receiver.0).await
    }
}

#[async_trait::async_trait]
impl Task for VmConcurrencyBarrier {
    fn id(&self) -> TaskId {
        "vm_concurrency_barrier_task".into()
    }

    async fn run(mut self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        // Wait for a stop request.
        stop_receiver.0.changed().await?;
        // Stop request was received: seal the barrier so that no new VM requests are accepted.
        self.close();
        // Wait until all the existing API requests are processed.
        // We don't have to synchronize this with API servers being stopped, as they can decide themselves how to handle
        // ongoing requests during the shutdown.
        // We don't have to implement a timeout here either, as it'll be handled by the framework itself.
        self.wait_until_stopped().await;
        Ok(())
    }
}

#[derive(Debug)]
pub struct WhitelistedTokensForAaUpdateTask {
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

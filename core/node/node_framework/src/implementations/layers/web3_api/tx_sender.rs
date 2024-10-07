use std::{sync::Arc, time::Duration};

use tokio::sync::RwLock;
use zksync_node_api_server::{
    execution_sandbox::{VmConcurrencyBarrier, VmConcurrencyLimiter},
    tx_sender::{SandboxExecutorOptions, TxSenderBuilder, TxSenderConfig},
};
use zksync_state::{PostgresStorageCaches, PostgresStorageCachesTask};
use zksync_types::{AccountTreeId, Address};
use zksync_web3_decl::{
    client::{DynClient, L2},
    jsonrpsee,
    namespaces::EnNamespaceClient as _,
};

use crate::{
    implementations::resources::{
        fee_input::ApiFeeInputResource,
        main_node_client::MainNodeClientResource,
        pools::{PoolResource, ReplicaPool},
        state_keeper::ConditionalSealerResource,
        web3_api::{TxSenderResource, TxSinkResource},
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
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
    tx_sender_config: TxSenderConfig,
    postgres_storage_caches_config: PostgresStorageCachesConfig,
    max_vm_concurrency: usize,
    whitelisted_tokens_for_aa_cache: bool,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub tx_sink: TxSinkResource,
    pub replica_pool: PoolResource<ReplicaPool>,
    pub fee_input: ApiFeeInputResource,
    pub main_node_client: Option<MainNodeClientResource>,
    pub sealer: Option<ConditionalSealerResource>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub tx_sender: TxSenderResource,
    #[context(task)]
    pub vm_concurrency_barrier: VmConcurrencyBarrier,
    #[context(task)]
    pub postgres_storage_caches_task: Option<PostgresStorageCachesTask>,
    #[context(task)]
    pub whitelisted_tokens_for_aa_update_task: Option<WhitelistedTokensForAaUpdateTask>,
}

impl TxSenderLayer {
    pub fn new(
        tx_sender_config: TxSenderConfig,
        postgres_storage_caches_config: PostgresStorageCachesConfig,
        max_vm_concurrency: usize,
    ) -> Self {
        Self {
            tx_sender_config,
            postgres_storage_caches_config,
            max_vm_concurrency,
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
            Some(update_task)
        } else {
            None
        };

        // Initialize `VmConcurrencyLimiter`.
        let (vm_concurrency_limiter, vm_concurrency_barrier) =
            VmConcurrencyLimiter::new(self.max_vm_concurrency);

        // TODO (BFT-138): Allow to dynamically reload API contracts
        let config = self.tx_sender_config;
        let executor_options = SandboxExecutorOptions::new(
            config.chain_id,
            AccountTreeId::new(config.fee_account_addr),
            config.validation_computational_gas_limit,
        )
        .await?;

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

        Ok(Output {
            tx_sender: tx_sender.into(),
            postgres_storage_caches_task,
            vm_concurrency_barrier,
            whitelisted_tokens_for_aa_update_task,
        })
    }
}

#[async_trait::async_trait]
impl Task for PostgresStorageCachesTask {
    fn id(&self) -> TaskId {
        "postgres_storage_caches".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

#[async_trait::async_trait]
impl Task for VmConcurrencyBarrier {
    fn id(&self) -> TaskId {
        "vm_concurrency_barrier_task".into()
    }

    async fn run(mut self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        // Wait for the stop signal.
        stop_receiver.0.changed().await?;
        // Stop signal was received: seal the barrier so that no new VM requests are accepted.
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

use std::{fmt, sync::Arc};

use zksync_core::api_server::{
    execution_sandbox::{VmConcurrencyBarrier, VmConcurrencyLimiter},
    tx_sender::{ApiContracts, TxSenderBuilder, TxSenderConfig},
};
use zksync_state::PostgresStorageCaches;

use crate::{
    implementations::resources::{
        fee_input::FeeInputResource,
        pools::ReplicaPoolResource,
        state_keeper::ConditionalSealerResource,
        web3_api::{TxSenderResource, TxSinkResource},
    },
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct PostgresStorageCachesConfig {
    pub factory_deps_cache_size: u64,
    pub initial_writes_cache_size: u64,
    pub latest_values_cache_size: u64,
}

#[derive(Debug)]
pub struct TxSenderLayer {
    tx_sender_config: TxSenderConfig,
    postgres_storage_caches_config: PostgresStorageCachesConfig,
    max_vm_concurrency: usize,
    api_contracts: ApiContracts,
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
        }
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
        let pool_resource = context.get_resource::<ReplicaPoolResource>().await?;
        let replica_pool = pool_resource.get().await?;
        let sealer = match context.get_resource::<ConditionalSealerResource>().await {
            Ok(sealer) => Some(sealer.0),
            Err(WiringError::ResourceLacking(_)) => None,
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
            let values_cache_task = storage_caches.configure_storage_values_cache(
                values_capacity,
                replica_pool.clone(),
                context.runtime_handle().clone(),
            );
            context.add_task(Box::new(PostgresStorageCachesTask {
                task: Box::new(values_cache_task),
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
        let tx_sender = tx_sender
            .build(
                fee_input,
                Arc::new(vm_concurrency_limiter),
                self.api_contracts,
                storage_caches,
            )
            .await;
        context.insert_resource(TxSenderResource(tx_sender))?;

        Ok(())
    }
}

struct PostgresStorageCachesTask {
    task: Box<dyn FnOnce() -> anyhow::Result<()> + Send>,
}

impl fmt::Debug for PostgresStorageCachesTask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PostgresStorageCachesTask")
            .finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl Task for PostgresStorageCachesTask {
    fn name(&self) -> &'static str {
        "postgres_storage_caches"
    }

    async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
        tokio::task::spawn_blocking(self.task).await?
    }
}

struct VmConcurrencyBarrierTask {
    barrier: VmConcurrencyBarrier,
}

#[async_trait::async_trait]
impl Task for VmConcurrencyBarrierTask {
    fn name(&self) -> &'static str {
        "vm_concurrency_barrier_task"
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

use zksync_consistency_checker::ConsistencyChecker;
use zksync_types::{commitment::L1BatchCommitmentMode, Address};

use crate::{
    implementations::resources::{
        eth_interface::EthInterfaceResource,
        healthcheck::AppHealthCheckResource,
        pools::{MasterPool, PoolResource},
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for the `ConsistencyChecker` (used by the external node).
///
/// ## Requests resources
///
/// - `EthInterfaceResource`
/// - `PoolResource<MasterPool>`
/// - `AppHealthCheckResource` (adds a health check)
///
/// ## Adds tasks
///
/// - `ConsistencyChecker`
#[derive(Debug)]
pub struct ConsistencyCheckerLayer {
    diamond_proxy_addr: Address,
    max_batches_to_recheck: u32,
    commitment_mode: L1BatchCommitmentMode,
}

impl ConsistencyCheckerLayer {
    pub fn new(
        diamond_proxy_addr: Address,
        max_batches_to_recheck: u32,
        commitment_mode: L1BatchCommitmentMode,
    ) -> ConsistencyCheckerLayer {
        Self {
            diamond_proxy_addr,
            max_batches_to_recheck,
            commitment_mode,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for ConsistencyCheckerLayer {
    fn layer_name(&self) -> &'static str {
        "consistency_checker_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        // Get resources.
        let l1_client = context.get_resource::<EthInterfaceResource>()?.0;

        let pool_resource = context.get_resource::<PoolResource<MasterPool>>()?;
        let singleton_pool = pool_resource.get_singleton().await?;

        let consistency_checker = ConsistencyChecker::new(
            l1_client,
            self.max_batches_to_recheck,
            singleton_pool,
            self.commitment_mode,
        )
        .map_err(WiringError::Internal)?
        .with_diamond_proxy_addr(self.diamond_proxy_addr);

        let AppHealthCheckResource(app_health) = context.get_resource_or_default();
        app_health
            .insert_component(consistency_checker.health_check().clone())
            .map_err(WiringError::internal)?;

        // Create and add tasks.
        context.add_task(Box::new(consistency_checker));

        Ok(())
    }
}

#[async_trait::async_trait]
impl Task for ConsistencyChecker {
    fn id(&self) -> TaskId {
        "consistency_checker".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

use zksync_core::consistency_checker::ConsistencyChecker;
use zksync_types::Address;

use crate::{
    implementations::resources::{
        eth_interface::EthInterfaceResource, healthcheck::AppHealthCheckResource,
        l1_batch_commit_data_generator::L1BatchCommitDataGeneratorResource,
        pools::MasterPoolResource,
    },
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct ConsistencyCheckerLayer {
    diamond_proxy_addr: Address,
    max_batches_to_recheck: u32,
}

impl ConsistencyCheckerLayer {
    pub fn new(
        diamond_proxy_addr: Address,
        max_batches_to_recheck: u32,
    ) -> ConsistencyCheckerLayer {
        Self {
            diamond_proxy_addr,
            max_batches_to_recheck,
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
        let l1_client = context.get_resource::<EthInterfaceResource>().await?.0;

        let pool_resource = context.get_resource::<MasterPoolResource>().await?;
        let singleton_pool = pool_resource.get_singleton().await?;

        let l1_batch_commit_data_generator = context
            .get_resource::<L1BatchCommitDataGeneratorResource>()
            .await?
            .0;

        let consistency_checker = ConsistencyChecker::new(
            l1_client,
            self.max_batches_to_recheck,
            singleton_pool,
            l1_batch_commit_data_generator,
        )
        .map_err(WiringError::Internal)?
        .with_diamond_proxy_addr(self.diamond_proxy_addr);

        let AppHealthCheckResource(app_health) = context.get_resource_or_default().await;
        app_health.insert_component(consistency_checker.health_check().clone());

        // Create and add tasks.
        context.add_task(Box::new(ConsistencyCheckerTask {
            consistency_checker,
        }));

        Ok(())
    }
}

pub struct ConsistencyCheckerTask {
    consistency_checker: ConsistencyChecker,
}

#[async_trait::async_trait]
impl Task for ConsistencyCheckerTask {
    fn name(&self) -> &'static str {
        "consistency_checker"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.consistency_checker.run(stop_receiver.0).await
    }
}

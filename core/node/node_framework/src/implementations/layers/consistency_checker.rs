use zksync_consistency_checker::ConsistencyChecker;
use zksync_types::{commitment::L1BatchCommitmentMode, Address, L1BatchNumber};

use crate::{
    implementations::resources::{
        eth_interface::{EthInterfaceResource, GatewayEthInterfaceResource},
        healthcheck::AppHealthCheckResource,
        pools::{MasterPool, PoolResource},
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for the `ConsistencyChecker` (used by the external node).
#[derive(Debug)]
pub struct ConsistencyCheckerLayer {
    diamond_proxy_addr: Address,
    max_batches_to_recheck: u32,
    commitment_mode: L1BatchCommitmentMode,
    migration_details: Option<(L1BatchNumber, Address)>,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub l1_client: EthInterfaceResource,
    pub master_pool: PoolResource<MasterPool>,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
    pub migration_client: Option<GatewayEthInterfaceResource>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub consistency_checker: ConsistencyChecker,
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
            migration_details: None,
        }
    }

    pub fn with_migration_details(
        mut self,
        first_batch_migrated: L1BatchNumber,
        diamond_proxy_address: Address,
    ) -> Self {
        self.migration_details = Some((first_batch_migrated, diamond_proxy_address));
        self
    }
}

#[async_trait::async_trait]
impl WiringLayer for ConsistencyCheckerLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "consistency_checker_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        // Get resources.
        let l1_client = input.l1_client.0;

        let singleton_pool = input.master_pool.get_singleton().await?;

        let mut consistency_checker = ConsistencyChecker::new(
            l1_client,
            self.max_batches_to_recheck,
            singleton_pool,
            self.commitment_mode,
        )
        .map_err(WiringError::Internal)?
        .with_diamond_proxy_addr(self.diamond_proxy_addr);

        if let Some((first_batch_migrated, diamond_proxy_address)) = self.migration_details {
            consistency_checker = consistency_checker.with_migration_setup(
                input
                    .migration_client
                    .expect("Migration client not provided")
                    .0,
                first_batch_migrated,
                Some(diamond_proxy_address),
            );
        }

        input
            .app_health
            .0
            .insert_component(consistency_checker.health_check().clone())
            .map_err(WiringError::internal)?;

        Ok(Output {
            consistency_checker,
        })
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

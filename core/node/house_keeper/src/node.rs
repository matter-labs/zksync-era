use zksync_config::configs::{house_keeper::HouseKeeperConfig, AirbenderProofDataHandlerConfig};
use zksync_dal::node::{PoolResource, ReplicaPool};
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_shared_resources::contracts::L1ChainContractsResource;
use zksync_web3_decl::client::{DynClient, L1};

use crate::{
    blocks_state_reporter::BlockMetricsReporter, periodic_job::PeriodicJob,
    two_factor_approval_reporter::TwoFactorApprovalReporter,
};

/// Wiring layer for `HouseKeeper` - a component responsible for managing prover jobs
/// and auxiliary server activities.
#[derive(Debug)]
pub struct HouseKeeperLayer {
    house_keeper_config: HouseKeeperConfig,
    airbender_config: Option<AirbenderProofDataHandlerConfig>,
}

#[derive(Debug, FromContext)]
pub struct Input {
    replica_pool: PoolResource<ReplicaPool>,
    l1_contracts: L1ChainContractsResource,
    eth_client: Box<DynClient<L1>>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub l1_batch_metrics_reporter: BlockMetricsReporter,
    #[context(task)]
    pub two_factor_approval_reporter: TwoFactorApprovalReporter,
}

impl HouseKeeperLayer {
    pub fn new(
        house_keeper_config: HouseKeeperConfig,
        airbender_config: Option<AirbenderProofDataHandlerConfig>,
    ) -> Self {
        Self {
            house_keeper_config,
            airbender_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for HouseKeeperLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "house_keeper_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        // Initialize resources
        let replica_pool = input.replica_pool.get().await?;

        // Initialize and add tasks
        let first_airbender_batch = self
            .airbender_config
            .as_ref()
            .map(|c| c.first_processed_batch)
            .unwrap_or_default();
        // Falls back to the config default when Airbender isn't configured; the value is unused in
        // that case since there are no Airbender jobs to count.
        let airbender_max_proving_attempts = self
            .airbender_config
            .as_ref()
            .map(|c| c.max_proving_attempts)
            .unwrap_or(10);

        let l1_batch_metrics_reporter = BlockMetricsReporter::new(
            self.house_keeper_config.l1_batch_metrics_reporting_interval,
            replica_pool.clone(),
            first_airbender_batch,
            airbender_max_proving_attempts,
        );

        let validator_timelock_addr = input
            .l1_contracts
            .0
            .ecosystem_contracts
            .validator_timelock_addr;
        let two_factor_approval_reporter = TwoFactorApprovalReporter::new(
            self.house_keeper_config.two_factor_approval_reporting_interval,
            replica_pool,
            input.eth_client,
            validator_timelock_addr,
        );

        Ok(Output {
            l1_batch_metrics_reporter,
            two_factor_approval_reporter,
        })
    }
}

#[async_trait::async_trait]
impl Task for BlockMetricsReporter {
    fn id(&self) -> TaskId {
        "l1_batch_metrics_reporter".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

#[async_trait::async_trait]
impl Task for TwoFactorApprovalReporter {
    fn id(&self) -> TaskId {
        "two_factor_approval_reporter".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

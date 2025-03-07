use zksync_consistency_checker::ConsistencyChecker;
use zksync_types::commitment::L1BatchCommitmentMode;

use crate::{
    implementations::resources::{
        contracts::SettlementLayerContractsResource,
        eth_interface::UniversalClientResource,
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
    max_batches_to_recheck: u32,
    commitment_mode: L1BatchCommitmentMode,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub gateway_client: UniversalClientResource,
    pub sl_chain_contracts: SettlementLayerContractsResource,
    pub master_pool: PoolResource<MasterPool>,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub consistency_checker: ConsistencyChecker,
}

impl ConsistencyCheckerLayer {
    pub fn new(
        max_batches_to_recheck: u32,
        commitment_mode: L1BatchCommitmentMode,
    ) -> ConsistencyCheckerLayer {
        Self {
            max_batches_to_recheck,
            commitment_mode,
        }
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
        let gateway_client = input.gateway_client.0;

        let singleton_pool = input.master_pool.get_singleton().await?;

        let consistency_checker = ConsistencyChecker::new(
            gateway_client.into(),
            self.max_batches_to_recheck,
            singleton_pool,
            self.commitment_mode,
        )
        .await
        .map_err(WiringError::Internal)?
        .with_l1_diamond_proxy_addr(
            input
                .sl_chain_contracts
                .0
                .chain_contracts_config
                .diamond_proxy_addr,
        );

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

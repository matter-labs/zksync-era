use zksync_consistency_checker::ConsistencyChecker;
use zksync_types::{commitment::L1BatchCommitmentMode, Address, L2ChainId};

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
    l1_diamond_proxy_addr: Address,
    max_batches_to_recheck: u32,
    commitment_mode: L1BatchCommitmentMode,
    l2_chain_id: L2ChainId,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub l1_client: EthInterfaceResource,
    pub gateway_client: Option<GatewayEthInterfaceResource>,
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
        l1_diamond_proxy_addr: Address,
        max_batches_to_recheck: u32,
        commitment_mode: L1BatchCommitmentMode,
        l2_chain_id: L2ChainId,
    ) -> ConsistencyCheckerLayer {
        Self {
            l1_diamond_proxy_addr,
            max_batches_to_recheck,
            commitment_mode,
            l2_chain_id,
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
        // Get resources.
        let l1_client = input.l1_client.0;
        let gateway_client = input.gateway_client.map(|c| c.0);

        let singleton_pool = input.master_pool.get_singleton().await?;

        let consistency_checker = ConsistencyChecker::new(
            l1_client,
            gateway_client,
            self.max_batches_to_recheck,
            singleton_pool,
            self.commitment_mode,
            self.l2_chain_id,
        )
        .await
        .map_err(WiringError::Internal)?
        .with_l1_diamond_proxy_addr(self.l1_diamond_proxy_addr);

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

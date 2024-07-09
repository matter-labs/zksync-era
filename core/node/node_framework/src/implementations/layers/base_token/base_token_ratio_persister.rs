use zksync_base_token_adjuster::BaseTokenRatioPersister;
use zksync_config::{configs::base_token_adjuster::BaseTokenAdjusterConfig, ContractsConfig};

use crate::{
    implementations::resources::{
        pools::{MasterPool, PoolResource},
        price_api_client::PriceAPIClientResource,
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for `BaseTokenRatioPersister`
///
/// Responsible for orchestrating communications with external API feeds to get ETH<->BaseToken
/// conversion ratios and persisting them both in the DB and in the L1.
#[derive(Debug)]
pub struct BaseTokenRatioPersisterLayer {
    config: BaseTokenAdjusterConfig,
    contracts_config: ContractsConfig,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    #[context(default)]
    pub price_api_client: PriceAPIClientResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub persister: BaseTokenRatioPersister,
}

impl BaseTokenRatioPersisterLayer {
    pub fn new(config: BaseTokenAdjusterConfig, contracts_config: ContractsConfig) -> Self {
        Self {
            config,
            contracts_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for BaseTokenRatioPersisterLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "base_token_ratio_persister"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let master_pool = input.master_pool.get().await?;

        let price_api_client = input.price_api_client;
        let base_token_addr = self
            .contracts_config
            .base_token_addr
            .expect("base token address is not set");

        let persister = BaseTokenRatioPersister::new(
            master_pool,
            self.config,
            base_token_addr,
            price_api_client.0,
        );

        Ok(Output { persister })
    }
}

#[async_trait::async_trait]
impl Task for BaseTokenRatioPersister {
    fn id(&self) -> TaskId {
        "base_token_ratio_persister".into()
    }

    async fn run(mut self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await?;
        Ok(())
    }
}

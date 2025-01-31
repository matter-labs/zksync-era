use anyhow::anyhow;
use zksync_config::{
    configs::{chain::StateKeeperConfig, da_dispatcher::DADispatcherConfig},
    ContractsConfig,
};
use zksync_da_dispatcher::DataAvailabilityDispatcher;
use zksync_eth_client::EthInterface;
use zksync_types::{ethabi, web3::CallRequest, Address};
use zksync_web3_decl::client::{DynClient, L1};

use crate::{
    implementations::resources::{
        da_client::DAClientResource,
        eth_interface::EthInterfaceResource,
        pools::{MasterPool, PoolResource},
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// A layer that wires the data availability dispatcher task.
#[derive(Debug)]
pub struct DataAvailabilityDispatcherLayer {
    state_keeper_config: StateKeeperConfig,
    da_config: DADispatcherConfig,
    contracts_config: ContractsConfig,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub eth_client: EthInterfaceResource,
    pub da_client: DAClientResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub da_dispatcher_task: DataAvailabilityDispatcher,
}

impl DataAvailabilityDispatcherLayer {
    pub fn new(
        state_keeper_config: StateKeeperConfig,
        da_config: DADispatcherConfig,
        contracts_config: ContractsConfig,
    ) -> Self {
        Self {
            state_keeper_config,
            da_config,
            contracts_config,
        }
    }

    async fn fetch_l1_da_validator_address(
        &self,
        eth_client: Box<DynClient<L1>>,
    ) -> Result<Address, WiringError> {
        let signature = ethabi::short_signature("getDAValidatorPair", &[]);
        let response = eth_client
            .call_contract_function(
                CallRequest {
                    data: Some(signature.into()),
                    to: Some(self.contracts_config.diamond_proxy_addr),
                    ..CallRequest::default()
                },
                None,
            )
            .await
            .map_err(|err| {
                WiringError::Internal(anyhow!("Failed to call the DA validator getter: {}", err))
            })?;

        let validators = ethabi::decode(
            &[ethabi::ParamType::Address, ethabi::ParamType::Address],
            response.0.as_slice(),
        )
        .map_err(|err| {
            WiringError::Internal(anyhow!(
                "Failed to decode the DA validator address: {}",
                err
            ))
        })?;

        validators[0]
            .clone()
            .into_address()
            .ok_or(WiringError::Internal(anyhow!(
                "Failed to decode the DA validator address".to_string()
            )))
    }
}

#[async_trait::async_trait]
impl WiringLayer for DataAvailabilityDispatcherLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "da_dispatcher_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let da_client = input.da_client.0;
        if let Some(limit) = da_client.blob_size_limit() {
            if self.state_keeper_config.max_pubdata_per_batch > limit as u64 {
                return Err(WiringError::Configuration(format!(
                    "Max pubdata per batch is greater than the blob size limit: {} > {}",
                    self.state_keeper_config.max_pubdata_per_batch, limit
                )));
            }
        }

        if let Some(no_da_validator) = self.contracts_config.no_da_validium_l1_validator_addr {
            let l1_da_validator_address = self
                .fetch_l1_da_validator_address(input.eth_client.0)
                .await?;

            if l1_da_validator_address != no_da_validator
                && self.da_config.use_dummy_inclusion_data()
            {
                return Err(WiringError::Configuration(format!(
                    "Dummy inclusion data is enabled, but not the NoDAValidator is used: {:?} != {:?}",
                    l1_da_validator_address, no_da_validator
                )));
            }
        }

        // A pool with size 2 is used here because there are 2 functions within a task that execute in parallel
        let master_pool = input.master_pool.get_custom(2).await?;

        let da_dispatcher_task = DataAvailabilityDispatcher::new(
            master_pool,
            self.da_config,
            da_client,
            self.contracts_config,
        );

        Ok(Output { da_dispatcher_task })
    }
}

#[async_trait::async_trait]
impl Task for DataAvailabilityDispatcher {
    fn id(&self) -> TaskId {
        "da_dispatcher".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

use std::sync::Arc;

use zksync_node_fee_model::l1_gas_price::MainNodeFeeParamsFetcher;

use crate::{
    implementations::resources::{
        fee_input::FeeInputResource, main_node_client::MainNodeClientResource,
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for main node fee params fetcher -- a fee input resource used on
/// the external node.
#[derive(Debug)]
pub struct MainNodeFeeParamsFetcherLayer;

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub main_node_client: MainNodeClientResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub fee_input: FeeInputResource,
    #[context(task)]
    pub fetcher: MainNodeFeeParamsFetcherTask,
}

#[async_trait::async_trait]
impl WiringLayer for MainNodeFeeParamsFetcherLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "main_node_fee_params_fetcher_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let MainNodeClientResource(main_node_client) = input.main_node_client;
        let fetcher = Arc::new(MainNodeFeeParamsFetcher::new(main_node_client));
        Ok(Output {
            fee_input: fetcher.clone().into(),
            fetcher: MainNodeFeeParamsFetcherTask { fetcher },
        })
    }
}

#[derive(Debug)]
pub struct MainNodeFeeParamsFetcherTask {
    fetcher: Arc<MainNodeFeeParamsFetcher>,
}

#[async_trait::async_trait]
impl Task for MainNodeFeeParamsFetcherTask {
    fn id(&self) -> TaskId {
        "main_node_fee_params_fetcher".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.fetcher.run(stop_receiver.0).await
    }
}

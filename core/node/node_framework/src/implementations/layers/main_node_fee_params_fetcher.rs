use std::sync::Arc;

use zksync_node_fee_model::l1_gas_price::MainNodeFeeParamsFetcher;

use crate::{
    implementations::resources::{
        fee_input::FeeInputResource, main_node_client::MainNodeClientResource,
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for main node fee params fetcher -- a fee input resource used on
/// the external node.
///
/// ## Requests resources
///
/// - `MainNodeClientResource`
///
/// ## Adds resources
///
/// - `FeeInputResource`
///
/// ## Adds tasks
///
/// - `MainNodeFeeParamsFetcherTask`
#[derive(Debug)]
pub struct MainNodeFeeParamsFetcherLayer;

#[async_trait::async_trait]
impl WiringLayer for MainNodeFeeParamsFetcherLayer {
    fn layer_name(&self) -> &'static str {
        "main_node_fee_params_fetcher_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let MainNodeClientResource(main_node_client) = context.get_resource()?;
        let fetcher = Arc::new(MainNodeFeeParamsFetcher::new(main_node_client));
        context.insert_resource(FeeInputResource(fetcher.clone()))?;
        context.add_task(Box::new(MainNodeFeeParamsFetcherTask { fetcher }));
        Ok(())
    }
}

#[derive(Debug)]
struct MainNodeFeeParamsFetcherTask {
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

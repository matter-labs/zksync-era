use std::sync::Arc;

use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_web3_decl::client::{DynClient, L2};

use super::resources::{ApiFeeInputResource, SequencerFeeInputResource};
use crate::l1_gas_price::MainNodeFeeParamsFetcher;

/// Wiring layer for main node fee params fetcher -- a fee input resource used on
/// the external node.
#[derive(Debug)]
pub struct MainNodeFeeParamsFetcherLayer;

#[derive(Debug, FromContext)]
pub struct Input {
    main_node_client: Box<DynClient<L2>>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    sequencer_fee_input: SequencerFeeInputResource,
    api_fee_input: ApiFeeInputResource,
    #[context(task)]
    fetcher: MainNodeFeeParamsFetcherTask,
}

#[async_trait::async_trait]
impl WiringLayer for MainNodeFeeParamsFetcherLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "main_node_fee_params_fetcher_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let fetcher = Arc::new(MainNodeFeeParamsFetcher::new(input.main_node_client));
        Ok(Output {
            sequencer_fee_input: fetcher.clone().into(),
            api_fee_input: fetcher.clone().into(),
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

use std::fmt::Debug;

use zksync_da_client::{node::DAClientResource, DataAvailabilityClient};
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

use crate::no_da::NoDAClient;

#[derive(Debug, Default)]
pub struct NoDAClientWiringLayer;

#[derive(Debug, IntoContext)]
pub struct Output {
    pub client: DAClientResource,
}

#[async_trait::async_trait]
impl WiringLayer for NoDAClientWiringLayer {
    type Input = ();
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "no_da_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        let client: Box<dyn DataAvailabilityClient> = Box::new(NoDAClient);

        Ok(Output {
            client: DAClientResource(client),
        })
    }
}

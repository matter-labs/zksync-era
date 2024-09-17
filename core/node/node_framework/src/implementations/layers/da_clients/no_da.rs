use std::fmt::Debug;

use zksync_da_client::DataAvailabilityClient;
use zksync_da_clients::no_da::NoDAClient;

use crate::{
    implementations::resources::da_client::DAClientResource,
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

#[derive(Debug, Default)]
pub struct NoDAClientWiringLayer;

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
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

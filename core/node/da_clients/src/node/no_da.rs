use std::fmt::Debug;

use zksync_da_client::DataAvailabilityClient;
use zksync_node_framework::wiring_layer::{WiringError, WiringLayer};

use crate::no_da::NoDAClient;

#[derive(Debug, Default)]
pub struct NoDAClientWiringLayer;

#[async_trait::async_trait]
impl WiringLayer for NoDAClientWiringLayer {
    type Input = ();
    type Output = Box<dyn DataAvailabilityClient>;

    fn layer_name(&self) -> &'static str {
        "no_da_layer"
    }

    async fn wire(self, (): Self::Input) -> Result<Self::Output, WiringError> {
        Ok(Box::new(NoDAClient))
    }
}

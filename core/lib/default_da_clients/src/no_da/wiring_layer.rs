use std::fmt::Debug;

use zksync_da_client::DataAvailabilityClient;
use zksync_node_framework::{
    implementations::resources::da_client::DAClientResource,
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

use crate::no_da::client::NoDAClient;

#[derive(Debug, Default)]
pub struct NoDAClientWiringLayer;

#[async_trait::async_trait]
impl WiringLayer for NoDAClientWiringLayer {
    fn layer_name(&self) -> &'static str {
        "no_da_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let client: Box<dyn DataAvailabilityClient> = Box::new(NoDAClient);

        context.insert_resource(DAClientResource(client))?;

        Ok(())
    }
}

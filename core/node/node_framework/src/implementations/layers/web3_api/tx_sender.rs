use zksync_config::ObjectStoreConfig;
use zksync_object_store::ObjectStoreFactory;

use crate::{
    implementations::resources::object_store::ObjectStoreResource,
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct TxSenderLayer {
    config: ObjectStoreConfig,
}

impl TxSenderLayer {
    pub fn new(config: ObjectStoreConfig) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl WiringLayer for TxSenderLayer {
    fn layer_name(&self) -> &'static str {
        "tx_sender_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let object_store = ObjectStoreFactory::new(self.config).create_store().await;
        context.insert_resource(ObjectStoreResource(object_store))?;
        Ok(())
    }
}

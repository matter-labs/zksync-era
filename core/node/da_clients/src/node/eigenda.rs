use std::{error::Error, sync::Arc};

use zksync_config::{configs::da_client::eigenda::EigenDASecrets, EigenDAConfig};
use zksync_da_client::DataAvailabilityClient;
use zksync_dal::{
    node::{MasterPool, PoolResource},
    ConnectionPool, Core, CoreDal,
};
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext,
};

use crate::eigen_da::{BlobProvider, EigenDAClient};

#[derive(Debug)]
pub struct EigenWiringLayer {
    config: EigenDAConfig,
    secrets: EigenDASecrets,
}

impl EigenWiringLayer {
    pub fn new(config: EigenDAConfig, secrets: EigenDASecrets) -> Self {
        Self { config, secrets }
    }
}

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
}

#[async_trait::async_trait]
impl WiringLayer for EigenWiringLayer {
    type Input = Input;
    type Output = Box<dyn DataAvailabilityClient>;

    fn layer_name(&self) -> &'static str {
        "eigenda_client_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let master_pool = input.master_pool.get().await?;
        let get_blob_from_db = GetBlobFromDB { pool: master_pool };
        let client =
            EigenDAClient::new(self.config, self.secrets, Arc::new(get_blob_from_db)).await?;
        Ok(Box::new(client))
    }
}

#[derive(Debug, Clone)]
pub struct GetBlobFromDB {
    pool: ConnectionPool<Core>,
}

#[async_trait::async_trait]
impl BlobProvider for GetBlobFromDB {
    async fn get_blob(&self, input: &str) -> Result<Option<Vec<u8>>, Box<dyn Error + Send + Sync>> {
        let mut conn = self.pool.connection_tagged("da_eigenda_client").await?;
        let batch = conn
            .data_availability_dal()
            .get_blob_data_by_blob_id(input)
            .await?;
        Ok(batch.map(|b| b.pubdata))
    }
}

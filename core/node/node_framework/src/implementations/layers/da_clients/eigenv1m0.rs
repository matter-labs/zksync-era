use std::{error::Error, sync::Arc};

use zksync_config::{configs::da_client::eigenv1m0::EigenSecretsV1M0, EigenConfigV1M0};
use zksync_da_client::DataAvailabilityClient;
use zksync_da_clients::eigenv1m0::{BlobProvider, EigenDAClientV1M0};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_node_framework_derive::FromContext;

use crate::{
    implementations::resources::{
        da_client::DAClientResource,
        pools::{MasterPool, PoolResource},
    },
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

#[derive(Debug)]
pub struct EigenV1M0WiringLayer {
    config: EigenConfigV1M0,
    secrets: EigenSecretsV1M0,
}

impl EigenV1M0WiringLayer {
    pub fn new(config: EigenConfigV1M0, secrets: EigenSecretsV1M0) -> Self {
        Self { config, secrets }
    }
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub client: DAClientResource,
}

#[async_trait::async_trait]
impl WiringLayer for EigenV1M0WiringLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "eigenv1m0_client_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let master_pool = input.master_pool.get().await?;
        let get_blob_from_db = GetBlobFromDB { pool: master_pool };
        let client: Box<dyn DataAvailabilityClient> = Box::new(
            EigenDAClientV1M0::new(self.config, self.secrets, Arc::new(get_blob_from_db)).await?,
        );

        Ok(Self::Output {
            client: DAClientResource(client),
        })
    }
}

#[derive(Debug, Clone)]
pub struct GetBlobFromDB {
    pool: ConnectionPool<Core>,
}

#[async_trait::async_trait]
impl BlobProvider for GetBlobFromDB {
    async fn get_blob(&self, input: &str) -> Result<Option<Vec<u8>>, Box<dyn Error + Send + Sync>> {
        let mut conn = self.pool.connection_tagged("eigenv1m0_client").await?;
        let batch = conn
            .data_availability_dal()
            .get_blob_data_by_blob_id(input)
            .await?;
        Ok(batch.map(|b| b.pubdata))
    }
}

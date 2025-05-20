use std::{error::Error, sync::Arc};

<<<<<<<< HEAD:core/node/da_clients/src/node/eigenda.rs
use zksync_config::{configs::da_client::eigenda::EigenDASecrets, EigenDAConfig};
use zksync_da_client::DataAvailabilityClient;
use zksync_da_clients::eigen_da::{BlobProvider, EigenDAClient};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_node_framework_derive::FromContext;

use crate::{
    implementations::resources::{
        da_client::DAClientResource,
        pools::{MasterPool, PoolResource},
    },
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
|||||||| 4905261f00:core/node/node_framework/src/implementations/layers/da_clients/eigen.rs
use zksync_config::{configs::da_client::eigen::EigenSecrets, EigenConfig};
use zksync_da_client::DataAvailabilityClient;
use zksync_da_clients::eigen::{BlobProvider, EigenDAClient};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_node_framework_derive::FromContext;

use crate::{
    implementations::resources::{
        da_client::DAClientResource,
        pools::{MasterPool, PoolResource},
    },
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
========
use zksync_config::{configs::da_client::eigen::EigenSecrets, EigenConfig};
use zksync_da_client::{node::DAClientResource, DataAvailabilityClient};
use zksync_dal::{
    node::{MasterPool, PoolResource},
    ConnectionPool, Core, CoreDal,
>>>>>>>> main:core/node/da_clients/src/node/eigen.rs
};
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

use crate::eigen::{BlobProvider, EigenDAClient};

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
    pub master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    pub client: DAClientResource,
}

#[async_trait::async_trait]
impl WiringLayer for EigenWiringLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "eigenda_client_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let master_pool = input.master_pool.get().await?;
        let get_blob_from_db = GetBlobFromDB { pool: master_pool };
        let client: Box<dyn DataAvailabilityClient> = Box::new(
            EigenDAClient::new(self.config, self.secrets, Arc::new(get_blob_from_db)).await?,
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
        let mut conn = self.pool.connection_tagged("da_eigenda_client").await?;
        let batch = conn
            .data_availability_dal()
            .get_blob_data_by_blob_id(input)
            .await?;
        Ok(batch.map(|b| b.pubdata))
    }
}

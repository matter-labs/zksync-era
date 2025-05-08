<<<<<<<< HEAD:core/node/node_framework/src/implementations/layers/da_clients/eigenv2m1.rs
use zksync_config::{configs::da_client::eigenv2m1::EigenSecretsV2M1, EigenConfigV2M1};
|||||||| 1c8c3a573:core/node/node_framework/src/implementations/layers/da_clients/eigen.rs
use zksync_config::{configs::da_client::eigen::EigenSecrets, EigenConfig};
========
use zksync_config::{configs::da_client::eigenv2m0::EigenSecretsV2M0, EigenConfigV2M0};
>>>>>>>> eigenda-v2-m0:core/node/node_framework/src/implementations/layers/da_clients/eigenv2m0.rs
use zksync_da_client::DataAvailabilityClient;
<<<<<<<< HEAD:core/node/node_framework/src/implementations/layers/da_clients/eigenv2m1.rs
use zksync_da_clients::eigenv2m1::EigenDAClientV2M1;
|||||||| 1c8c3a573:core/node/node_framework/src/implementations/layers/da_clients/eigen.rs
use zksync_da_clients::eigen::EigenDAClient;
========
use zksync_da_clients::eigenv2m0::EigenDAClientV2M0;
>>>>>>>> eigenda-v2-m0:core/node/node_framework/src/implementations/layers/da_clients/eigenv2m0.rs
use zksync_node_framework_derive::FromContext;

use crate::{
    implementations::resources::da_client::DAClientResource,
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

#[derive(Debug)]
<<<<<<<< HEAD:core/node/node_framework/src/implementations/layers/da_clients/eigenv2m1.rs
pub struct EigenV2M1WiringLayer {
    config: EigenConfigV2M1,
    secrets: EigenSecretsV2M1,
|||||||| 1c8c3a573:core/node/node_framework/src/implementations/layers/da_clients/eigen.rs
pub struct EigenWiringLayer {
    config: EigenConfig,
    secrets: EigenSecrets,
========
pub struct EigenV2M0WiringLayer {
    config: EigenConfigV2M0,
    secrets: EigenSecretsV2M0,
>>>>>>>> eigenda-v2-m0:core/node/node_framework/src/implementations/layers/da_clients/eigenv2m0.rs
}

<<<<<<<< HEAD:core/node/node_framework/src/implementations/layers/da_clients/eigenv2m1.rs
impl EigenV2M1WiringLayer {
    pub fn new(config: EigenConfigV2M1, secrets: EigenSecretsV2M1) -> Self {
|||||||| 1c8c3a573:core/node/node_framework/src/implementations/layers/da_clients/eigen.rs
impl EigenWiringLayer {
    pub fn new(config: EigenConfig, secrets: EigenSecrets) -> Self {
========
impl EigenV2M0WiringLayer {
    pub fn new(config: EigenConfigV2M0, secrets: EigenSecretsV2M0) -> Self {
>>>>>>>> eigenda-v2-m0:core/node/node_framework/src/implementations/layers/da_clients/eigenv2m0.rs
        Self { config, secrets }
    }
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub client: DAClientResource,
}

#[async_trait::async_trait]
<<<<<<<< HEAD:core/node/node_framework/src/implementations/layers/da_clients/eigenv2m1.rs
impl WiringLayer for EigenV2M1WiringLayer {
|||||||| 1c8c3a573:core/node/node_framework/src/implementations/layers/da_clients/eigen.rs
impl WiringLayer for EigenWiringLayer {
========
impl WiringLayer for EigenV2M0WiringLayer {
>>>>>>>> eigenda-v2-m0:core/node/node_framework/src/implementations/layers/da_clients/eigenv2m0.rs
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
<<<<<<<< HEAD:core/node/node_framework/src/implementations/layers/da_clients/eigenv2m1.rs
        "eigenv2m1_client_layer"
|||||||| 1c8c3a573:core/node/node_framework/src/implementations/layers/da_clients/eigen.rs
        "eigen_client_layer"
========
        "eigenv2m0_client_layer"
>>>>>>>> eigenda-v2-m0:core/node/node_framework/src/implementations/layers/da_clients/eigenv2m0.rs
    }

    async fn wire(self, _: Self::Input) -> Result<Self::Output, WiringError> {
        let client: Box<dyn DataAvailabilityClient> =
<<<<<<<< HEAD:core/node/node_framework/src/implementations/layers/da_clients/eigenv2m1.rs
            Box::new(EigenDAClientV2M1::new(self.config, self.secrets).await?);
|||||||| 1c8c3a573:core/node/node_framework/src/implementations/layers/da_clients/eigen.rs
            Box::new(EigenDAClient::new(self.config, self.secrets).await?);
========
            Box::new(EigenDAClientV2M0::new(self.config, self.secrets).await?);
>>>>>>>> eigenda-v2-m0:core/node/node_framework/src/implementations/layers/da_clients/eigenv2m0.rs

        Ok(Self::Output {
            client: DAClientResource(client),
        })
    }
}

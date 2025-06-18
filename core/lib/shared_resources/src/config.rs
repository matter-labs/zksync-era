use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use zksync_config::ParsedParam;
use zksync_health_check::{AppHealthCheck, CheckHealth, Health, HealthStatus};
use zksync_node_framework::{FromContext, WiringError, WiringLayer};

#[derive(Debug, FromContext)]
pub struct Input {
    #[context(default)]
    app_health: Arc<AppHealthCheck>,
}

#[derive(Debug)]
pub struct ConfigParamsLayer(pub HashMap<String, ParsedParam>);

#[async_trait]
impl WiringLayer for ConfigParamsLayer {
    type Input = Input;
    type Output = ();

    fn layer_name(&self) -> &'static str {
        "common/parsed_params_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let health_check = Arc::new(self);
        input
            .app_health
            .insert_custom_component(health_check)
            .map_err(WiringError::internal)?;
        Ok(())
    }
}

#[async_trait]
impl CheckHealth for ConfigParamsLayer {
    fn name(&self) -> &'static str {
        "config"
    }

    async fn check_health(&self) -> Health {
        Health::from(HealthStatus::Ready).with_details(&self.0)
    }
}

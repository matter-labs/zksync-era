use zksync_config::configs::gateway::GatewayChainConfig;

use crate::Resource;

#[derive(Debug, Clone)]
pub struct ContractsResource(pub GatewayChainConfig);

impl Resource for ContractsResource {
    fn name() -> String {
        "common/contracts".into()
    }
}

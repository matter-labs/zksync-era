use zksync_config::configs::contracts::Contracts;

use crate::Resource;

#[derive(Debug, Clone)]
pub struct ContractsResource(pub Contracts);

impl Resource for ContractsResource {
    fn name() -> String {
        "common/contracts".into()
    }
}

use zksync_node_framework::resource::Resource;

use crate::DataAvailabilityClient;

/// Represents a client of a certain DA solution.
#[derive(Debug, Clone)]
pub struct DAClientResource(pub Box<dyn DataAvailabilityClient>);

impl Resource for DAClientResource {
    fn name() -> String {
        "common/da_client".into()
    }
}

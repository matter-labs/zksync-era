use zksync_da_client::DataAvailabilityClient;

use crate::resource::Resource;

/// Represents a client of a certain DA solution.
#[derive(Debug, Clone)]
pub struct DAClientResource(pub Box<dyn DataAvailabilityClient>);

impl Resource for DAClientResource {
    fn name() -> String {
        "common/da_client".into()
    }
}

pub struct TransitionalDAClientResource(pub Box<dyn DataAvailabilityClient>);

impl Resource for TransitionalDAClientResource {
    fn name() -> String {
        "common/transitional_da_client".into()
    }
}

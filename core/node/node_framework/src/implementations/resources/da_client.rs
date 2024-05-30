use zksync_da_layers::DataAvailabilityClient;

use crate::resource::Resource;

/// Represents a client of a certain DA solution.
#[derive(Clone)]
pub struct DAClientResource(pub Box<dyn DataAvailabilityClient>);

impl Resource for DAClientResource {
    fn name() -> String {
        "common/da_client".into()
    }
}

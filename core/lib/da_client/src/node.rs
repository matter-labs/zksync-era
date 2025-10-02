use zksync_node_framework::resource::{self, Resource};

use crate::DataAvailabilityClient;

impl Resource<resource::Boxed> for dyn DataAvailabilityClient {
    fn name() -> String {
        "common/da_client".into()
    }
}

use zksync_node_framework::resource::Resource;

use crate::DataAvailabilityClient;

impl Resource for Box<dyn DataAvailabilityClient> {
    fn name() -> String {
        "common/da_client".into()
    }
}

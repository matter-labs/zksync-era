use zksync_node_framework::{resource, Resource};

use crate::PriceApiClient;

impl Resource<resource::Shared> for dyn PriceApiClient {
    fn name() -> String {
        "common/price_api_client".into()
    }
}

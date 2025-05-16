use zksync_node_framework::Resource;

use crate::PriceApiClient;

impl Resource for dyn PriceApiClient {
    fn name() -> String {
        "common/price_api_client".into()
    }
}

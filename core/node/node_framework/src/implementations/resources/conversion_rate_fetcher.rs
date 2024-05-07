use std::sync::Arc;

use crate::resource::Resource;
use zksync_base_token_fetcher::ConversionRateFetcher;

#[derive(Debug, Clone)]
pub struct ConversionRateFetcherResource(pub Arc<dyn ConversionRateFetcher>);

impl Resource for ConversionRateFetcherResource {
    fn name() -> String {
        "common/conversion_rate_fetcher".into()
    }
}

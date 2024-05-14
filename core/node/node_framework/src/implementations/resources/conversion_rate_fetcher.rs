use std::sync::Arc;

use zksync_core::base_token_fetcher::ConversionRateFetcher;

use crate::resource::Resource;

#[derive(Debug, Clone)]
pub struct ConversionRateFetcherResource(pub Arc<dyn ConversionRateFetcher>);

impl Resource for ConversionRateFetcherResource {
    fn name() -> String {
        "common/conversion_rate_fetcher".into()
    }
}

use std::sync::Arc;

use zksync_base_token_adjuster::BaseTokenFetcher;

use crate::resource::Resource;

/// A resource that provides [`BaseTokenFetcher`] implementation to the service.
#[derive(Clone, Default)]
pub struct BaseTokenFetcherResource(pub Arc<BaseTokenFetcher>);

impl Resource for BaseTokenFetcherResource {
    fn name() -> String {
        "common/base_token_fetcher".into()
    }
}

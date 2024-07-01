use std::sync::Arc;

use zksync_base_token_adjuster::{BaseTokenFetcher, NoOpFetcher};

use crate::resource::Resource;

/// A resource that provides [`BaseTokenFetcher`] implementation to the service.
#[derive(Clone)]
pub struct BaseTokenFetcherResource(pub Arc<dyn BaseTokenFetcher>);

impl Default for BaseTokenFetcherResource {
    fn default() -> Self {
        Self(Arc::new(NoOpFetcher::default()))
    }
}

impl Resource for BaseTokenFetcherResource {
    fn name() -> String {
        "common/base_token_fetcher".into()
    }
}

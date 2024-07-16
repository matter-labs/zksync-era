use std::sync::Arc;

use zksync_base_token_adjuster::{BaseTokenRatioProvider, NoOpRatioProvider};

use crate::resource::Resource;

/// A resource that provides [`BaseTokenRatioProvider`] implementation to the service.
#[derive(Debug, Clone)]
pub struct BaseTokenRatioProviderResource(pub Arc<dyn BaseTokenRatioProvider>);

impl Default for BaseTokenRatioProviderResource {
    fn default() -> Self {
        Self(Arc::new(NoOpRatioProvider::default()))
    }
}

impl Resource for BaseTokenRatioProviderResource {
    fn name() -> String {
        "common/base_token_ratio_provider".into()
    }
}

impl<T: BaseTokenRatioProvider> From<Arc<T>> for BaseTokenRatioProviderResource {
    fn from(provider: Arc<T>) -> Self {
        Self(provider)
    }
}

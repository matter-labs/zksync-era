use async_trait::async_trait;
use zksync_bin_metadata::BinMetadata;

use crate::{CheckHealth, Health, HealthStatus};

impl From<&BinMetadata> for Health {
    fn from(details: &BinMetadata) -> Self {
        Self::from(HealthStatus::Ready).with_details(details)
    }
}

#[async_trait]
impl CheckHealth for BinMetadata {
    fn name(&self) -> &'static str {
        "metadata"
    }

    async fn check_health(&self) -> Health {
        self.into()
    }
}

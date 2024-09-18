use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::proto::house_keeper as proto;

impl ProtoRepr for proto::HouseKeeper {
    type Type = configs::house_keeper::HouseKeeperConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            l1_batch_metrics_reporting_interval_ms: *required(
                &self.l1_batch_metrics_reporting_interval_ms,
            )
            .context("l1_batch_metrics_reporting_interval_ms")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            l1_batch_metrics_reporting_interval_ms: Some(
                this.l1_batch_metrics_reporting_interval_ms,
            ),
        }
    }
}

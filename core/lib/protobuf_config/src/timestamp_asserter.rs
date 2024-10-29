use anyhow::Context;
use zksync_config::configs::chain::TimestampAsserterConfig;
use zksync_protobuf::{required, ProtoRepr};

impl ProtoRepr for crate::proto::config::timestamp_asserter::TimestampAsserter {
    type Type = TimestampAsserterConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            min_time_till_end_sec: *required(&self.min_time_till_end_sec)
                .context("timestamp_asserter_min_time_till_end_sec")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            min_time_till_end_sec: Some(this.min_time_till_end_sec),
        }
    }
}

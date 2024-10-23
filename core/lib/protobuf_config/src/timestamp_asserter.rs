use zksync_config::configs::chain::TimestampAsserterConfig;
use zksync_protobuf::{required, ProtoRepr};

impl ProtoRepr for crate::proto::config::timestamp_asserter::TimestampAsserter {
    type Type = TimestampAsserterConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            min_range_sec: *required(&self.min_range_sec).unwrap_or(&0),
            min_time_till_end_sec: *required(&self.min_time_till_end_sec).unwrap_or(&0),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            min_range_sec: Some(this.min_range_sec),
            min_time_till_end_sec: Some(this.min_time_till_end_sec),
        }
    }
}

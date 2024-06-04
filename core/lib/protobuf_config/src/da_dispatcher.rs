use anyhow::Context;
use zksync_config::configs::{self};
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::da_dispatcher as proto;

impl ProtoRepr for proto::DataAvailabilityDispatcher {
    type Type = configs::da_dispatcher::DADispatcherConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(configs::da_dispatcher::DADispatcherConfig {
            polling_interval_ms: Some(
                *required(&self.polling_interval).context("polling_interval")?,
            ),
            query_rows_limit: Some(*required(&self.query_rows_limit).context("query_rows_limit")?),
            max_retries: Some(*required(&self.max_retries).context("query_rows_limit")? as u16),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            polling_interval: this.polling_interval_ms,
            query_rows_limit: this.query_rows_limit,
            max_retries: this.max_retries.map(|x| x as u32),
        }
    }
}

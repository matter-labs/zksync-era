use zksync_config::configs::{self};
use zksync_protobuf::ProtoRepr;

use crate::proto::da_dispatcher as proto;

impl ProtoRepr for proto::DataAvailabilityDispatcher {
    type Type = configs::da_dispatcher::DADispatcherConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(configs::da_dispatcher::DADispatcherConfig {
            polling_interval_ms: self.polling_interval_ms,
            max_rows_to_dispatch: self.max_rows_to_dispatch,
            max_retries: self.max_retries.map(|x| x as u16),
            use_dummy_inclusion_data: self.use_dummy_inclusion_data,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            polling_interval_ms: this.polling_interval_ms,
            max_rows_to_dispatch: this.max_rows_to_dispatch,
            max_retries: this.max_retries.map(Into::into),
            use_dummy_inclusion_data: this.use_dummy_inclusion_data,
        }
    }
}

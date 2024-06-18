use zksync_types::{zk_evm_types::LogQuery, StorageLog};

use crate::glue::{GlueFrom, GlueInto};

impl<T: GlueInto<LogQuery>> GlueFrom<T> for StorageLog {
    fn glue_from(value: T) -> Self {
        StorageLog::from_log_query(&value.glue_into())
    }
}

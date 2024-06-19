use zksync_types::{
    zk_evm_types::LogQuery, StorageLog, StorageLogQuery, StorageLogWithPreviousValue,
};
use zksync_utils::u256_to_h256;

use crate::glue::{GlueFrom, GlueInto};

impl<T: GlueInto<LogQuery>> GlueFrom<T> for StorageLog {
    fn glue_from(value: T) -> Self {
        StorageLog::from_log_query(&value.glue_into())
    }
}

impl<T: GlueInto<StorageLogQuery>> GlueFrom<T> for StorageLogWithPreviousValue {
    fn glue_from(value: T) -> Self {
        let query = value.glue_into();
        StorageLogWithPreviousValue {
            log: StorageLog {
                kind: query.log_type,
                ..StorageLog::from_log_query(&query.log_query)
            },
            previous_value: u256_to_h256(if query.log_query.rollback {
                query.log_query.written_value
            } else {
                query.log_query.read_value
            }),
        }
    }
}

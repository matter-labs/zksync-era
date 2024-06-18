use zksync_types::{zk_evm_types::LogQuery, StorageLog, StorageLogQuery};

use crate::glue::{GlueFrom, GlueInto};

impl<T: GlueInto<LogQuery>> GlueFrom<T> for StorageLog {
    fn glue_from(value: T) -> Self {
        StorageLog::from_log_query(&value.glue_into())
    }
}

pub(crate) fn storage_log_from_storage_log_query<T>(value: T) -> StorageLog
where
    T: GlueInto<StorageLogQuery>,
{
    let storage_log_query = value.glue_into();
    StorageLog {
        kind: storage_log_query.log_type,
        ..storage_log_query.log_query.into()
    }
}

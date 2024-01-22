use zk_evm_1_5_0::{
    aux_structures::{LogQuery as LogQuery_1_5_0, Timestamp as Timestamp_1_5_0},
    zkevm_opcode_defs::FarCallOpcode as FarCallOpcode_1_5_0,
};
use zksync_types::{FarCallOpcode, LogQuery, Timestamp};

use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<FarCallOpcode_1_5_0> for FarCallOpcode {
    fn glue_from(value: FarCallOpcode_1_5_0) -> Self {
        match value {
            FarCallOpcode_1_5_0::Normal => FarCallOpcode::Normal,
            FarCallOpcode_1_5_0::Delegate => FarCallOpcode::Delegate,
            FarCallOpcode_1_5_0::Mimic => FarCallOpcode::Mimic,
        }
    }
}

impl GlueFrom<Timestamp_1_5_0> for Timestamp {
    fn glue_from(value: Timestamp_1_5_0) -> Timestamp {
        Timestamp(value.0)
    }
}

impl GlueFrom<Timestamp> for Timestamp_1_5_0 {
    fn glue_from(value: Timestamp) -> Timestamp_1_5_0 {
        Timestamp_1_5_0(value.0)
    }
}

impl GlueFrom<LogQuery_1_5_0> for LogQuery {
    fn glue_from(value: LogQuery_1_5_0) -> LogQuery {
        LogQuery {
            timestamp: value.timestamp.glue_into(),
            tx_number_in_block: value.tx_number_in_block,
            aux_byte: value.aux_byte,
            shard_id: value.shard_id,
            address: value.address,
            key: value.key,
            read_value: value.read_value,
            written_value: value.written_value,
            rw_flag: value.rw_flag,
            rollback: value.rollback,
            is_service: value.is_service,
        }
    }
}

impl GlueFrom<LogQuery> for LogQuery_1_5_0 {
    fn glue_from(value: LogQuery) -> LogQuery_1_5_0 {
        LogQuery_1_5_0 {
            timestamp: value.timestamp.glue_into(),
            tx_number_in_block: value.tx_number_in_block,
            aux_byte: value.aux_byte,
            shard_id: value.shard_id,
            address: value.address,
            key: value.key,
            read_value: value.read_value,
            written_value: value.written_value,
            rw_flag: value.rw_flag,
            rollback: value.rollback,
            is_service: value.is_service,
        }
    }
}

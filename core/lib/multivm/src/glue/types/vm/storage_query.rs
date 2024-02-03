use zksync_types::StorageLogQuery;

use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<crate::vm_m5::utils::StorageLogQuery> for StorageLogQuery {
    fn glue_from(value: crate::vm_m5::utils::StorageLogQuery) -> Self {
        Self {
            log_query: value.log_query.glue_into(),
            log_type: value.log_type,
        }
    }
}

impl GlueFrom<crate::vm_m6::utils::StorageLogQuery> for StorageLogQuery {
    fn glue_from(value: crate::vm_m6::utils::StorageLogQuery) -> Self {
        Self {
            log_query: value.log_query.glue_into(),
            log_type: value.log_type,
        }
    }
}

impl GlueFrom<crate::vm_1_3_2::utils::StorageLogQuery> for StorageLogQuery {
    fn glue_from(value: crate::vm_1_3_2::utils::StorageLogQuery) -> Self {
        Self {
            log_query: value.log_query.glue_into(),
            log_type: value.log_type,
        }
    }
}

impl GlueFrom<crate::vm_virtual_blocks::utils::logs::StorageLogQuery> for StorageLogQuery {
    fn glue_from(value: crate::vm_virtual_blocks::utils::logs::StorageLogQuery) -> Self {
        Self {
            log_query: value.log_query.glue_into(),
            log_type: value.log_type,
        }
    }
}

impl GlueFrom<crate::vm_refunds_enhancement::utils::logs::StorageLogQuery> for StorageLogQuery {
    fn glue_from(value: crate::vm_refunds_enhancement::utils::logs::StorageLogQuery) -> Self {
        Self {
            log_query: value.log_query.glue_into(),
            log_type: value.log_type,
        }
    }
}

impl GlueFrom<crate::vm_boojum_integration::utils::logs::StorageLogQuery> for StorageLogQuery {
    fn glue_from(value: crate::vm_boojum_integration::utils::logs::StorageLogQuery) -> Self {
        Self {
            log_query: value.log_query.glue_into(),
            log_type: value.log_type,
        }
    }
}

impl GlueFrom<crate::vm_latest::utils::logs::StorageLogQuery> for StorageLogQuery {
    fn glue_from(value: crate::vm_latest::utils::logs::StorageLogQuery) -> Self {
        Self {
            log_query: value.log_query.glue_into(),
            log_type: value.log_type,
        }
    }
}

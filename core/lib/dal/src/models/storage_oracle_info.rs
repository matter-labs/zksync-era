use zksync_types::block::StorageOracleInfo;

/// Projection of the `l1_batches` table corresponding to [`L1BatchHeader`].
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct DbStorageOracleInfo {
    pub storage_refunds: Option<Vec<i64>>,
    pub pubdata_costs: Option<Vec<i64>>,
}

impl DbStorageOracleInfo {
    pub(crate) fn into_optional_batch_oracle_info(self) -> Option<StorageOracleInfo> {
        let DbStorageOracleInfo {
            storage_refunds,
            pubdata_costs,
        } = self;

        let storage_refunds: Vec<u32> = storage_refunds.map(|refunds| {
            // Here we do `.try_into().unwrap()` to ensure consistency of the data
            refunds
                .into_iter()
                .map(|refund| refund.try_into().unwrap())
                .collect()
        })?;

        let pubdata_costs = pubdata_costs.map(|costs| {
            // Here we do `.try_into().unwrap()` to ensure consistency of the data
            costs
                .into_iter()
                .map(|cost| cost.try_into().unwrap())
                .collect()
        });

        Some(StorageOracleInfo {
            storage_refunds,
            pubdata_costs,
        })
    }
}

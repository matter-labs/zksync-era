use serde::{Deserialize, Serialize};
use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::{AccountTreeId, StorageKey, StorageLog, H160, H256};

use crate::Core;

#[derive(Debug)]
pub struct CustomGenesisExportDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisState {
    pub storage_logs: Vec<StorageLogRow>,
    pub factory_deps: Vec<FactoryDepRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageLogRow {
    pub address: [u8; 20],
    pub key: [u8; 32],
    pub value: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactoryDepRow {
    pub bytecode_hash: [u8; 32],
    pub bytecode: Vec<u8>,
}

impl CustomGenesisExportDal<'_, '_> {
    pub async fn get_storage_logs(&mut self) -> DalResult<Vec<StorageLogRow>> {
        // This method returns storage logs that are used for genesis export.
        //
        // The where clause with addresses filters out SystemContext related records
        // 0x0 -- chainId,
        // 0x3 -- blockGasLimit,
        // 0x4 -- coinbase,
        // 0x5 -- difficulty
        let rows = sqlx::query!(
            r#"
            WITH latest_storage_logs AS (
                SELECT DISTINCT ON (hashed_key)
                    hashed_key,
                    address,
                    key,
                    value
                FROM storage_logs
                ORDER BY hashed_key, miniblock_number DESC, operation_number DESC
            )
            
            SELECT
                lsl.address,
                lsl.key,
                lsl.value
            FROM
                initial_writes iw
            JOIN
                latest_storage_logs lsl ON iw.hashed_key = lsl.hashed_key
            WHERE
                lsl.value
                <> '\x0000000000000000000000000000000000000000000000000000000000000000'::bytea
                AND (
                    lsl.address <> '\x000000000000000000000000000000000000800b'::bytea OR
                    lsl.key IN (
                        '\x0000000000000000000000000000000000000000000000000000000000000000'::bytea,
                        '\x0000000000000000000000000000000000000000000000000000000000000003'::bytea,
                        '\x0000000000000000000000000000000000000000000000000000000000000004'::bytea,
                        '\x0000000000000000000000000000000000000000000000000000000000000005'::bytea
                    )
                );
            "#,
        )
        .instrument("get_storage_logs")
        .fetch_all(self.storage)
        .await?;

        let storage_logs: Vec<StorageLogRow> = rows
            .into_iter()
            .map(|row| StorageLogRow {
                address: row.address.unwrap().try_into().unwrap(),
                key: row.key.unwrap().try_into().unwrap(),
                value: row.value.try_into().unwrap(),
            })
            .collect();

        Ok(storage_logs)
    }

    pub async fn get_factory_deps(&mut self) -> DalResult<Vec<FactoryDepRow>> {
        // 1. Fetch the rows from the database
        let rows = sqlx::query!(
            r#"
            SELECT
                bytecode_hash AS "bytecode_hash!",
                bytecode AS "bytecode!"
            FROM factory_deps
            "#
        )
        .instrument("get_factory_deps")
        .fetch_all(self.storage)
        .await?;

        // 2. Map the rows to FactoryDepRow structs
        let factory_deps: Vec<FactoryDepRow> = rows
            .into_iter()
            .map(|row| FactoryDepRow {
                bytecode_hash: row.bytecode_hash.try_into().unwrap(),
                bytecode: row.bytecode,
            })
            .collect();

        Ok(factory_deps)
    }
}

impl From<&StorageLogRow> for StorageLog {
    fn from(value: &StorageLogRow) -> Self {
        StorageLog::new_write_log(
            StorageKey::new(AccountTreeId::new(H160(value.address)), H256(value.key)),
            H256(value.value),
        )
    }
}

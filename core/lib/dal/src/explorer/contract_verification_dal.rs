use std::time::Duration;

use zksync_types::{
    explorer_api::{
        DeployContractCalldata, VerificationIncomingRequest, VerificationInfo, VerificationRequest,
        VerificationRequestStatus,
    },
    get_code_key, Address, CONTRACT_DEPLOYER_ADDRESS, FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH,
};

use sqlx::postgres::types::PgInterval;

use crate::SqlxError;
use crate::StorageProcessor;

#[derive(Debug)]
pub struct ContractVerificationDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl ContractVerificationDal<'_, '_> {
    pub fn get_count_of_queued_verification_requests(&mut self) -> Result<usize, SqlxError> {
        async_std::task::block_on(async {
            sqlx::query!(
                r#"
                SELECT COUNT(*) as "count!"
                FROM contract_verification_requests
                WHERE status = 'queued'
                "#
            )
            .fetch_one(self.storage.conn())
            .await
            .map(|row| row.count as usize)
        })
    }

    pub fn add_contract_verification_request(
        &mut self,
        query: VerificationIncomingRequest,
    ) -> Result<usize, SqlxError> {
        async_std::task::block_on(async {
            sqlx::query!(
                "
                INSERT INTO contract_verification_requests (
                    contract_address,
                    source_code,
                    contract_name,
                    compiler_zksolc_version,
                    compiler_solc_version,
                    optimization_used,
                    constructor_arguments,
                    is_system,
                    status,
                    created_at,
                    updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'queued', now(), now())
                RETURNING id
                ",
                query.contract_address.as_bytes(),
                serde_json::to_string(&query.source_code_data).unwrap(),
                query.contract_name,
                query.compiler_zksolc_version,
                query.compiler_solc_version,
                query.optimization_used,
                query.constructor_arguments.0,
                query.is_system,
            )
            .fetch_one(self.storage.conn())
            .await
            .map(|row| row.id as usize)
        })
    }

    /// Returns the next verification request for processing.
    /// Considering the situation where processing of some request
    /// can be interrupted (panic, pod restart, etc..),
    /// `processing_timeout` parameter is added to avoid stucking of requests.
    pub fn get_next_queued_verification_request(
        &mut self,
        processing_timeout: Duration,
    ) -> Result<Option<VerificationRequest>, SqlxError> {
        async_std::task::block_on(async {
            let processing_timeout = PgInterval {
                months: 0,
                days: 0,
                microseconds: processing_timeout.as_micros() as i64,
            };
            let result = sqlx::query!(
                "UPDATE contract_verification_requests
                SET status = 'in_progress', attempts = attempts + 1,
                    updated_at = now(), processing_started_at = now()
                WHERE id = (
                    SELECT id FROM contract_verification_requests
                    WHERE status = 'queued' OR (status = 'in_progress' AND processing_started_at < now() - $1::interval)
                    ORDER BY created_at
                    LIMIT 1
                    FOR UPDATE
                    SKIP LOCKED
                )
                RETURNING contract_verification_requests.*",
                &processing_timeout
            )
            .fetch_optional(self.storage.conn())
            .await?
            .map(|row| VerificationRequest {
                id: row.id as usize,
                req: VerificationIncomingRequest {
                    contract_address: Address::from_slice(&row.contract_address),
                    source_code_data: serde_json::from_str(&row.source_code).unwrap(),
                    contract_name: row.contract_name,
                    compiler_zksolc_version: row.compiler_zksolc_version,
                    compiler_solc_version: row.compiler_solc_version,
                    optimization_used: row.optimization_used,
                    constructor_arguments: row.constructor_arguments.into(),
                    is_system: row.is_system,
                },
            });
            Ok(result)
        })
    }

    /// Updates the verification request status and inserts the verification info upon successful verification.
    pub fn save_verification_info(
        &mut self,
        verification_info: VerificationInfo,
    ) -> Result<(), SqlxError> {
        async_std::task::block_on(async {
            let mut transaction = self.storage.start_transaction().await;

            sqlx::query!(
                "
                UPDATE contract_verification_requests
                SET status = 'successful', updated_at = now()
                WHERE id = $1
                ",
                verification_info.request.id as i64,
            )
            .execute(transaction.conn())
            .await?;

            let address = verification_info.request.req.contract_address;
            let verification_info_json = serde_json::to_value(verification_info)
                .expect("Failed to serialize verification info into serde_json");
            sqlx::query!(
                "
                    INSERT INTO contracts_verification_info
                    (address, verification_info)
                    VALUES ($1, $2)
                    ON CONFLICT (address)
                    DO UPDATE SET verification_info = $2
                ",
                address.as_bytes(),
                &verification_info_json
            )
            .execute(transaction.conn())
            .await?;

            transaction.commit().await;
            Ok(())
        })
    }

    pub fn save_verification_error(
        &mut self,
        id: usize,
        error: String,
        compilation_errors: serde_json::Value,
        panic_message: Option<String>,
    ) -> Result<(), SqlxError> {
        async_std::task::block_on(async {
            sqlx::query!(
                "
                UPDATE contract_verification_requests
                SET status = 'failed', updated_at = now(), error = $2, compilation_errors = $3, panic_message = $4
                WHERE id = $1
                ",
                id as i64,
                error.as_str(),
                &compilation_errors,
                panic_message
            )
            .execute(self.storage.conn())
            .await?;
            Ok(())
        })
    }

    pub fn get_verification_request_status(
        &mut self,
        id: usize,
    ) -> Result<Option<VerificationRequestStatus>, SqlxError> {
        async_std::task::block_on(async {
            let result = sqlx::query!(
                "
                SELECT status, error, compilation_errors FROM contract_verification_requests
                WHERE id = $1
                ",
                id as i64,
            )
            .fetch_optional(self.storage.conn())
            .await?
            .map(|row| VerificationRequestStatus {
                status: row.status,
                error: row.error,
                compilation_errors: row
                    .compilation_errors
                    .and_then(|errors: serde_json::Value| {
                        let string_array: Vec<String> = errors
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|value| value.as_str().unwrap().to_string())
                            .collect();
                        if string_array.is_empty() {
                            None
                        } else {
                            Some(string_array)
                        }
                    }),
            });
            Ok(result)
        })
    }

    /// Returns bytecode and calldata from the contract and the transaction that created it.
    pub fn get_contract_info_for_verification(
        &mut self,
        address: Address,
    ) -> Result<Option<(Vec<u8>, DeployContractCalldata)>, SqlxError> {
        async_std::task::block_on(async {
            let hashed_key = get_code_key(&address).hashed_key();
            let result = sqlx::query!(
                r#"
                    SELECT factory_deps.bytecode, transactions.data as "data?", transactions.contract_address as "contract_address?"
                    FROM (
                        SELECT * FROM storage_logs
                        WHERE storage_logs.hashed_key = $1
                        ORDER BY miniblock_number DESC, operation_number DESC
                        LIMIT 1
                    ) storage_logs
                    JOIN factory_deps ON factory_deps.bytecode_hash = storage_logs.value
                    LEFT JOIN transactions ON transactions.hash = storage_logs.tx_hash
                    WHERE storage_logs.value != $2
                "#,
                hashed_key.as_bytes(),
                FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH.as_bytes()
            )
            .fetch_optional(self.storage.conn())
            .await?
            .map(|row| {
                let calldata = match row.contract_address {
                    Some(contract_address)
                        if contract_address == CONTRACT_DEPLOYER_ADDRESS.0.to_vec() =>
                    {
                        // `row.contract_address` and `row.data` are either both `None` or both `Some(_)`.
                        // In this arm it's checked that `row.contract_address` is `Some(_)`, so it's safe to unwrap `row.data`.
                        let data: serde_json::Value = row.data.unwrap();
                        let calldata_str: String =
                            serde_json::from_value(data.get("calldata").unwrap().clone()).unwrap();
                        let calldata = hex::decode(&calldata_str[2..]).unwrap();
                        DeployContractCalldata::Deploy(calldata)
                    }
                    _ => DeployContractCalldata::Ignore,
                };
                (row.bytecode, calldata)
            });
            Ok(result)
        })
    }

    /// Returns true if the contract has a stored contracts_verification_info.
    pub fn is_contract_verified(&mut self, address: Address) -> bool {
        async_std::task::block_on(async {
            let count = sqlx::query!(
                r#"
                    SELECT COUNT(*) as "count!"
                    FROM contracts_verification_info
                    WHERE address = $1
                "#,
                address.as_bytes()
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap()
            .count;
            count > 0
        })
    }

    pub fn get_zksolc_versions(&mut self) -> Result<Vec<String>, SqlxError> {
        async_std::task::block_on(async {
            let versions: Vec<_> = sqlx::query!(
                "SELECT version FROM contract_verification_zksolc_versions ORDER by version"
            )
            .fetch_all(self.storage.conn())
            .await?
            .into_iter()
            .map(|row| row.version)
            .collect();
            Ok(versions)
        })
    }

    pub fn get_solc_versions(&mut self) -> Result<Vec<String>, SqlxError> {
        async_std::task::block_on(async {
            let versions: Vec<_> = sqlx::query!(
                "SELECT version FROM contract_verification_solc_versions ORDER by version"
            )
            .fetch_all(self.storage.conn())
            .await?
            .into_iter()
            .map(|row| row.version)
            .collect();
            Ok(versions)
        })
    }

    pub fn set_zksolc_versions(&mut self, versions: Vec<String>) -> Result<(), SqlxError> {
        async_std::task::block_on(async {
            let mut transaction = self.storage.start_transaction().await;

            sqlx::query!("DELETE FROM contract_verification_zksolc_versions")
                .execute(transaction.conn())
                .await?;

            sqlx::query!(
                "
                    INSERT INTO contract_verification_zksolc_versions (version, created_at, updated_at)
                    SELECT u.version, now(), now()
                        FROM UNNEST($1::text[])
                    AS u(version)
                ",
                &versions
            )
                .execute(transaction.conn())
                .await?;

            transaction.commit().await;
            Ok(())
        })
    }

    pub fn set_solc_versions(&mut self, versions: Vec<String>) -> Result<(), SqlxError> {
        async_std::task::block_on(async {
            let mut transaction = self.storage.start_transaction().await;

            sqlx::query!("DELETE FROM contract_verification_solc_versions")
                .execute(transaction.conn())
                .await?;

            sqlx::query!(
                "
                    INSERT INTO contract_verification_solc_versions (version, created_at, updated_at)
                    SELECT u.version, now(), now()
                        FROM UNNEST($1::text[])
                    AS u(version)
                ",
                &versions
            )
                .execute(transaction.conn())
                .await?;

            transaction.commit().await;
            Ok(())
        })
    }

    pub fn get_all_successful_requests(&mut self) -> Result<Vec<VerificationRequest>, SqlxError> {
        async_std::task::block_on(async {
            let result = sqlx::query!(
                "SELECT * FROM contract_verification_requests
                WHERE status = 'successful'
                ORDER BY id",
            )
            .fetch_all(self.storage.conn())
            .await?
            .into_iter()
            .map(|row| VerificationRequest {
                id: row.id as usize,
                req: VerificationIncomingRequest {
                    contract_address: Address::from_slice(&row.contract_address),
                    source_code_data: serde_json::from_str(&row.source_code).unwrap(),
                    contract_name: row.contract_name,
                    compiler_zksolc_version: row.compiler_zksolc_version,
                    compiler_solc_version: row.compiler_solc_version,
                    optimization_used: row.optimization_used,
                    constructor_arguments: row.constructor_arguments.into(),
                    is_system: row.is_system,
                },
            })
            .collect();
            Ok(result)
        })
    }
}

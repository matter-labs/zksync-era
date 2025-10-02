#![doc = include_str!("../doc/ContractVerificationDal.md")]

use std::{
    fmt::{Display, Formatter},
    time::Duration,
};

use rayon::prelude::*;
use sqlx::postgres::types::PgInterval;
use zksync_db_connection::{
    error::SqlxContext,
    instrument::{CopyStatement, InstrumentExt},
};
use zksync_types::{
    address_to_h256,
    contract_verification::{
        api::{
            VerificationIncomingRequest, VerificationInfo, VerificationRequest,
            VerificationRequestStatus,
        },
        contract_identifier::ContractIdentifier,
    },
    web3, Address, CONTRACT_DEPLOYER_ADDRESS, H256,
};
use zksync_vm_interface::VmEvent;

use crate::{
    models::storage_verification_request::StorageVerificationRequest, Connection, Core, DalResult,
};

#[derive(Debug)]
enum Compiler {
    ZkSolc,
    Solc,
    ZkVyper,
    Vyper,
}

impl Display for Compiler {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZkSolc => f.write_str("zksolc"),
            Self::Solc => f.write_str("solc"),
            Self::ZkVyper => f.write_str("zkvyper"),
            Self::Vyper => f.write_str("vyper"),
        }
    }
}

#[derive(Debug)]
pub struct DeployedContractData {
    pub bytecode_hash: H256,
    /// Bytecode as persisted in Postgres (i.e., with additional padding for EVM bytecodes).
    pub bytecode: Vec<u8>,
    /// Recipient of the deployment transaction.
    pub contract_address: Option<Address>,
    /// Call data for the deployment transaction.
    pub calldata: Option<Vec<u8>>,
}

#[derive(Debug)]
pub struct ContractVerificationDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl ContractVerificationDal<'_, '_> {
    pub async fn get_count_of_queued_verification_requests(&mut self) -> DalResult<usize> {
        sqlx::query!(
            r#"
            SELECT
                COUNT(*) AS "count!"
            FROM
                contract_verification_requests
            WHERE
                status = 'queued'
            "#
        )
        .instrument("get_count_of_queued_verification_requests")
        .fetch_one(self.storage)
        .await
        .map(|row| row.count as usize)
    }

    /// Returns ID of verification request for the specified contract address
    /// that wasn't processed yet.
    pub async fn get_active_verification_request(
        &mut self,
        address: Address,
    ) -> DalResult<Option<usize>> {
        sqlx::query!(
            r#"
            SELECT
                id
            FROM
                contract_verification_requests
            WHERE
                contract_address = $1 AND (status = 'queued' OR status = 'in_progress')
            LIMIT 1
            "#,
            address.as_bytes(),
        )
        .instrument("verification_request_exists")
        .fetch_optional(self.storage)
        .await
        .map(|row| row.map(|row| row.id as usize))
    }

    pub async fn add_contract_verification_request(
        &mut self,
        query: &VerificationIncomingRequest,
    ) -> DalResult<usize> {
        sqlx::query!(
            r#"
            INSERT INTO
            contract_verification_requests (
                contract_address,
                source_code,
                contract_name,
                zk_compiler_version,
                compiler_version,
                optimization_used,
                optimizer_mode,
                constructor_arguments,
                is_system,
                force_evmla,
                evm_specific,
                status,
                created_at,
                updated_at
            )
            VALUES
            ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'queued', NOW(), NOW())
            RETURNING
            id
            "#,
            query.contract_address.as_bytes(),
            // Serialization should always succeed.
            serde_json::to_string(&query.source_code_data).unwrap(),
            &query.contract_name,
            query.compiler_versions.zk_compiler_version(),
            query.compiler_versions.compiler_version(),
            query.optimization_used,
            query.optimizer_mode.as_deref(),
            query.constructor_arguments.0.as_slice(),
            query.is_system,
            query.force_evmla,
            serde_json::to_value(&query.evm_specific).unwrap(),
        )
        .instrument("add_contract_verification_request")
        .with_arg("address", &query.contract_address)
        .fetch_one(self.storage)
        .await
        .map(|row| row.id as usize)
    }

    /// Returns the next verification request for processing.
    /// Considering the situation where processing of some request
    /// can be interrupted (panic, pod restart, etc..),
    /// `processing_timeout` parameter is added to avoid stuck requests.
    pub async fn get_next_queued_verification_request(
        &mut self,
        processing_timeout: Duration,
    ) -> DalResult<Option<VerificationRequest>> {
        let processing_timeout = PgInterval {
            months: 0,
            days: 0,
            microseconds: processing_timeout.as_micros() as i64,
        };
        let result = sqlx::query_as!(
            StorageVerificationRequest,
            r#"
            UPDATE contract_verification_requests
            SET
                status = 'in_progress',
                attempts = attempts + 1,
                updated_at = NOW(),
                processing_started_at = NOW()
            WHERE
                id = (
                    SELECT
                        id
                    FROM
                        contract_verification_requests
                    WHERE
                        status = 'queued'
                        OR (
                            status = 'in_progress'
                            AND processing_started_at < NOW() - $1::INTERVAL
                        )
                    ORDER BY
                        created_at
                    LIMIT
                        1
                    FOR UPDATE
                    SKIP LOCKED
                )
            RETURNING
            id,
            contract_address,
            source_code,
            contract_name,
            zk_compiler_version,
            compiler_version,
            optimization_used,
            optimizer_mode,
            constructor_arguments,
            is_system,
            force_evmla,
            evm_specific
            "#,
            &processing_timeout
        )
        .instrument("get_next_queued_verification_request")
        .with_arg("processing_timeout", &processing_timeout)
        .fetch_optional(self.storage)
        .await?
        .map(Into::into);
        Ok(result)
    }

    /// Updates the verification request status and inserts the verification info upon successful verification.
    pub async fn save_verification_info(
        &mut self,
        verification_info: VerificationInfo,
        bytecode_keccak256: H256,
        bytecode_without_metadata_keccak256: H256,
    ) -> DalResult<()> {
        let mut transaction = self.storage.start_transaction().await?;
        let id = verification_info.request.id;
        let address = verification_info.request.req.contract_address;

        sqlx::query!(
            r#"
            UPDATE contract_verification_requests
            SET
                status = 'successful',
                updated_at = NOW()
            WHERE
                id = $1
            "#,
            verification_info.request.id as i64,
        )
        .instrument("save_verification_info#set_status")
        .with_arg("id", &id)
        .with_arg("address", &address)
        .execute(&mut transaction)
        .await?;

        // Serialization should always succeed.
        let verification_info_json = serde_json::to_value(verification_info)
            .expect("Failed to serialize verification info into serde_json");
        sqlx::query!(
            r#"
            INSERT INTO
            contract_verification_info_v2 (
                initial_contract_addr,
                bytecode_keccak256,
                bytecode_without_metadata_keccak256,
                verification_info
            )
            VALUES
            ($1, $2, $3, $4)
            ON CONFLICT (initial_contract_addr) DO
            UPDATE
            SET
            bytecode_keccak256 = $2,
            bytecode_without_metadata_keccak256 = $3,
            verification_info = $4
            "#,
            address.as_bytes(),
            bytecode_keccak256.as_bytes(),
            bytecode_without_metadata_keccak256.as_bytes(),
            &verification_info_json
        )
        .instrument("save_verification_info#insert")
        .with_arg("id", &id)
        .with_arg("address", &address)
        .execute(&mut transaction)
        .await?;

        transaction.commit().await
    }

    pub async fn save_verification_error(
        &mut self,
        id: usize,
        error: &str,
        compilation_errors: &serde_json::Value,
        panic_message: Option<&str>,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE contract_verification_requests
            SET
                status = 'failed',
                updated_at = NOW(),
                error = $2,
                compilation_errors = $3,
                panic_message = $4
            WHERE
                id = $1
            "#,
            id as i64,
            error,
            compilation_errors,
            panic_message
        )
        .instrument("save_verification_error")
        .with_arg("id", &id)
        .with_arg("error", &error)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn update_verification_request_compiler_versions(
        &mut self,
        id: usize,
        compiler_version: &str,
        zk_compiler_version: Option<&str>,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE contract_verification_requests
            SET
                compiler_version = $2,
                zk_compiler_version = $3,
                updated_at = NOW()
            WHERE
                id = $1
            "#,
            id as i64,
            compiler_version,
            zk_compiler_version,
        )
        .instrument("update_verification_request_compiler_versions")
        .with_arg("id", &id)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn get_verification_request_status(
        &mut self,
        id: usize,
    ) -> DalResult<Option<VerificationRequestStatus>> {
        sqlx::query!(
            r#"
            SELECT
                status,
                error,
                compilation_errors
            FROM
                contract_verification_requests
            WHERE
                id = $1
            "#,
            id as i64,
        )
        .try_map(|row| {
            let mut compilation_errors = vec![];
            if let Some(errors) = row.compilation_errors {
                let serde_json::Value::Array(errors) = errors else {
                    return Err(anyhow::anyhow!("errors are not an array"))
                        .decode_column("compilation_errors")?;
                };
                for value in errors {
                    let serde_json::Value::String(err) = value else {
                        return Err(anyhow::anyhow!("error is not a string"))
                            .decode_column("compilation_errors")?;
                    };
                    compilation_errors.push(err.to_owned());
                }
            }

            Ok(VerificationRequestStatus {
                status: row.status,
                error: row.error,
                compilation_errors: (!compilation_errors.is_empty()).then_some(compilation_errors),
            })
        })
        .instrument("get_verification_request_status")
        .with_arg("id", &id)
        .fetch_optional(self.storage)
        .await
    }

    /// Returns bytecode and calldata from the contract and the transaction that created it.
    pub async fn get_contract_info_for_verification(
        &mut self,
        address: Address,
    ) -> DalResult<Option<DeployedContractData>> {
        let address_h256 = address_to_h256(&address);
        sqlx::query!(
            r#"
            SELECT
                factory_deps.bytecode_hash,
                factory_deps.bytecode,
                transactions.data -> 'calldata' AS "calldata?",
                transactions.contract_address AS "contract_address?"
            FROM
                (
                    SELECT
                        miniblock_number,
                        tx_hash,
                        topic3
                    FROM
                        events
                    WHERE
                        address = $1
                        AND topic1 = $2
                        AND topic4 = $3
                    ORDER BY miniblock_number DESC, event_index_in_block DESC
                    LIMIT
                        1
                ) deploy_event
            JOIN factory_deps ON factory_deps.bytecode_hash = deploy_event.topic3
            LEFT JOIN transactions ON transactions.hash = deploy_event.tx_hash
            WHERE
                deploy_event.miniblock_number <= (
                    SELECT
                        MAX(number)
                    FROM
                        miniblocks
                )
            "#,
            CONTRACT_DEPLOYER_ADDRESS.as_bytes(),
            VmEvent::DEPLOY_EVENT_SIGNATURE.as_bytes(),
            address_h256.as_bytes(),
        )
        .try_map(|row| {
            Ok(DeployedContractData {
                bytecode_hash: H256::from_slice(&row.bytecode_hash),
                bytecode: row.bytecode,
                contract_address: row.contract_address.as_deref().map(Address::from_slice),
                calldata: row
                    .calldata
                    .map(|calldata| {
                        serde_json::from_value::<web3::Bytes>(calldata)
                            .decode_column("calldata")
                            .map(|bytes| bytes.0)
                    })
                    .transpose()?,
            })
        })
        .instrument("get_contract_info_for_verification")
        .with_arg("address", &address)
        .fetch_optional(self.storage)
        .await
    }

    async fn get_compiler_versions(&mut self, compiler: Compiler) -> DalResult<Vec<String>> {
        let compiler = format!("{compiler}");
        let versions: Vec<_> = sqlx::query!(
            r#"
            SELECT
                VERSION
            FROM
                COMPILER_VERSIONS
            WHERE
                COMPILER = $1
            ORDER BY
                VERSION
            "#,
            &compiler
        )
        .instrument("get_compiler_versions")
        .with_arg("compiler", &compiler)
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|row| row.version)
        .collect();
        Ok(versions)
    }

    pub async fn get_zksolc_versions(&mut self) -> DalResult<Vec<String>> {
        self.get_compiler_versions(Compiler::ZkSolc).await
    }

    pub async fn get_solc_versions(&mut self) -> DalResult<Vec<String>> {
        self.get_compiler_versions(Compiler::Solc).await
    }

    pub async fn get_zkvyper_versions(&mut self) -> DalResult<Vec<String>> {
        self.get_compiler_versions(Compiler::ZkVyper).await
    }

    pub async fn get_vyper_versions(&mut self) -> DalResult<Vec<String>> {
        self.get_compiler_versions(Compiler::Vyper).await
    }

    async fn set_compiler_versions(
        &mut self,
        compiler: Compiler,
        versions: &[String],
    ) -> DalResult<()> {
        let mut transaction = self.storage.start_transaction().await?;
        let compiler = format!("{compiler}");

        sqlx::query!(
            r#"
            DELETE FROM compiler_versions
            WHERE
                compiler = $1
            "#,
            &compiler
        )
        .instrument("set_compiler_versions#delete")
        .with_arg("compiler", &compiler)
        .execute(&mut transaction)
        .await?;

        sqlx::query!(
            r#"
            INSERT INTO
            compiler_versions (version, compiler, created_at, updated_at)
            SELECT
                u.version,
                $2,
                NOW(),
                NOW()
            FROM
                UNNEST($1::TEXT []) AS u (version)
            ON CONFLICT (version, compiler) DO NOTHING
            "#,
            versions,
            &compiler,
        )
        .instrument("set_compiler_versions#insert")
        .with_arg("compiler", &compiler)
        .with_arg("versions.len", &versions.len())
        .execute(&mut transaction)
        .await?;

        transaction.commit().await
    }

    pub async fn set_zksolc_versions(&mut self, versions: &[String]) -> DalResult<()> {
        self.set_compiler_versions(Compiler::ZkSolc, versions).await
    }

    pub async fn set_solc_versions(&mut self, versions: &[String]) -> DalResult<()> {
        self.set_compiler_versions(Compiler::Solc, versions).await
    }

    pub async fn set_zkvyper_versions(&mut self, versions: &[String]) -> DalResult<()> {
        self.set_compiler_versions(Compiler::ZkVyper, versions)
            .await
    }

    pub async fn set_vyper_versions(&mut self, versions: &[String]) -> DalResult<()> {
        self.set_compiler_versions(Compiler::Vyper, versions).await
    }

    pub async fn get_all_successful_requests(&mut self) -> DalResult<Vec<VerificationRequest>> {
        let result = sqlx::query_as!(
            StorageVerificationRequest,
            r#"
            SELECT
                id,
                contract_address,
                source_code,
                contract_name,
                zk_compiler_version,
                compiler_version,
                optimization_used,
                optimizer_mode,
                constructor_arguments,
                is_system,
                force_evmla,
                evm_specific
            FROM
                contract_verification_requests
            WHERE
                status = 'successful'
            ORDER BY
                id
            "#,
        )
        .instrument("get_all_successful_requests")
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(Into::into)
        .collect();
        Ok(result)
    }

    pub async fn get_contract_verification_info(
        &mut self,
        address: Address,
    ) -> anyhow::Result<Option<VerificationInfo>> {
        // Do everything in a read-only transaction for a consistent view.
        let mut transaction = self
            .storage
            .transaction_builder()?
            .set_readonly()
            .build()
            .await?;

        let mut dal = ContractVerificationDal {
            storage: &mut transaction,
        };
        let info = if dal.is_verification_info_migration_performed().await? {
            dal.get_contract_verification_info_v2(address).await?
        } else {
            dal.get_contract_verification_info_v1(address).await?
        };
        Ok(info)
    }

    async fn get_contract_verification_info_v1(
        &mut self,
        address: Address,
    ) -> DalResult<Option<VerificationInfo>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                verification_info
            FROM
                contracts_verification_info
            WHERE
                address = $1
            "#,
            address.as_bytes(),
        )
        .try_map(|row| {
            row.verification_info
                .map(|info| serde_json::from_value(info).decode_column("verification_info"))
                .transpose()
        })
        .instrument("get_contract_verification_info")
        .with_arg("address", &address)
        .fetch_optional(self.storage)
        .await?
        .flatten())
    }

    async fn get_contract_verification_info_v2(
        &mut self,
        address: Address,
    ) -> anyhow::Result<Option<VerificationInfo>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                verification_info
            FROM
                contract_verification_info_v2
            WHERE
                initial_contract_addr = $1
            "#,
            address.as_bytes(),
        )
        .try_map(|row| {
            serde_json::from_value(row.verification_info).decode_column("verification_info")
        })
        .instrument("get_contract_verification_info_v2")
        .with_arg("address", &address)
        .fetch_optional(self.storage)
        .await?
        .flatten())
    }

    /// Returns verification info for the contract.
    /// Tries to find the full bytecode match first. If it's not found, tries to find the partial match for the
    /// bytecode without metadata.
    pub async fn get_partial_match_verification_info(
        &mut self,
        bytecode_keccak256: H256,
        bytecode_without_metadata_keccak256: H256,
    ) -> DalResult<Option<(VerificationInfo, H256, H256)>> {
        // Use double select so the full match by bytecode_keccak256 is checked first.
        // Second query is for the partial match by bytecode_without_metadata_keccak256.
        // Aliases for the columns are needed to properly work with the UNION. Otherwise, the types will be of type
        // Option<T> instead of T. It's a known sqlx issue reported here: https://github.com/launchbadge/sqlx/issues/1266
        sqlx::query!(
            r#"
            (
                SELECT
                    verification_info AS "verification_info!",
                    bytecode_keccak256 AS "bytecode_keccak256!",
                    bytecode_without_metadata_keccak256 AS "bytecode_without_metadata_keccak256!"
                FROM
                    contract_verification_info_v2
                WHERE
                    bytecode_keccak256 = $1
                LIMIT 1
            )
            UNION ALL
            (
                SELECT
                    verification_info AS "verification_info!",
                    bytecode_keccak256 AS "bytecode_keccak256!",
                    bytecode_without_metadata_keccak256 AS "bytecode_without_metadata_keccak256!"
                FROM
                    contract_verification_info_v2
                WHERE
                    bytecode_without_metadata_keccak256 = $2
                LIMIT 1
            )
            LIMIT 1;
            "#,
            bytecode_keccak256.as_bytes(),
            bytecode_without_metadata_keccak256.as_bytes()
        )
        .try_map(|row| {
            let info = serde_json::from_value::<VerificationInfo>(row.verification_info)
                .decode_column("verification_info")?;
            let bytecode_keccak256 = H256::from_slice(&row.bytecode_keccak256);
            let bytecode_without_metadata_keccak256 =
                H256::from_slice(&row.bytecode_without_metadata_keccak256);
            Ok((
                info,
                bytecode_keccak256,
                bytecode_without_metadata_keccak256,
            ))
        })
        .instrument("get_partial_match_verification_info")
        .with_arg("bytecode_keccak256", &bytecode_keccak256)
        .with_arg(
            "bytecode_without_metadata_keccak256",
            &bytecode_without_metadata_keccak256,
        )
        .fetch_optional(self.storage)
        .await
    }

    /// Checks if migration from `contracts_verification_info` to `contract_verification_info_v2` is performed
    /// by checking if the latter has more or equal number of rows.
    pub async fn is_verification_info_migration_performed(&mut self) -> DalResult<bool> {
        let row = sqlx::query!(
            r#"
            SELECT
                (SELECT COUNT(*) FROM contracts_verification_info) AS count_v1,
                (SELECT COUNT(*) FROM contract_verification_info_v2) AS count_v2
            "#,
        )
        .instrument("is_verification_info_migration_performed")
        .fetch_one(self.storage)
        .await?;

        Ok(row.count_v2 >= row.count_v1)
    }

    pub async fn perform_verification_info_migration(
        &mut self,
        batch_size: usize,
    ) -> anyhow::Result<()> {
        // We use a long-running transaction, since the migration is one-time and during it
        // no writes are expected to the tables, so locked rows are not a problem.
        let mut transaction = self.storage.start_transaction().await?;

        // Offset is a number of already migrated contracts.
        let mut offset = 0usize;
        let mut cursor = vec![];
        loop {
            let cursor_str = format!("0x{}", hex::encode(&cursor));

            // Fetch JSON as text to avoid roundtrip through `serde_json::Value`, as it's super slow.
            let (addresses, verification_infos): (Vec<Vec<u8>>, Vec<String>) = sqlx::query!(
                r#"
                SELECT
                    address,
                    verification_info::text AS verification_info
                FROM
                    contracts_verification_info
                WHERE address > $1
                ORDER BY
                    address
                LIMIT $2
                "#,
                &cursor,
                batch_size as i64,
            )
            .instrument("perform_verification_info_migration#select")
            .with_arg("cursor", &cursor_str)
            .with_arg("batch_size", &batch_size)
            .fetch_all(&mut transaction)
            .await?
            .into_iter()
            .filter_map(|row| row.verification_info.map(|info| (row.address, info)))
            .collect();

            if addresses.is_empty() {
                tracing::info!("No more contracts to process");
                break;
            }

            tracing::info!(
                "Processing {} contracts (processed: {offset}); cursor {cursor_str}",
                addresses.len()
            );

            let ids: Vec<ContractIdentifier> = (0..addresses.len())
                .into_par_iter()
                .map(|idx| {
                    let address = &addresses[idx];
                    let info_json = &verification_infos[idx];
                    let verification_info = serde_json::from_str::<VerificationInfo>(info_json)
                        .unwrap_or_else(|err| {
                            panic!(
                                "Malformed data in DB, address {}, data: {info_json}, error: {err}",
                                hex::encode(address)
                            );
                        });
                    ContractIdentifier::from_bytecode(
                        verification_info.bytecode_marker(),
                        verification_info.artifacts.deployed_bytecode(),
                    )
                })
                .collect();

            let now = chrono::Utc::now().naive_utc().to_string();
            let mut buffer = String::new();
            for idx in 0..addresses.len() {
                let address = hex::encode(&addresses[idx]);
                let bytecode_keccak256 = hex::encode(ids[idx].bytecode_keccak256);
                let bytecode_without_metadata_keccak256 =
                    hex::encode(ids[idx].bytecode_without_metadata_keccak256);
                let verification_info = verification_infos[idx].replace('"', r#""""#);

                // Note: when using CSV format, you shouldn't escape backslashes, as they are not treated as escape characters.
                // If you will use `\\` here, it will be treated as a string instead of bytea.
                let row = format!(
                    r#"\x{initial_contract_addr},\x{bytecode_keccak256},\x{bytecode_without_metadata_keccak256},"{verification_info}",{created_at},{updated_at}"#,
                    initial_contract_addr = address,
                    bytecode_keccak256 = bytecode_keccak256,
                    bytecode_without_metadata_keccak256 = bytecode_without_metadata_keccak256,
                    verification_info = verification_info,
                    created_at = now,
                    updated_at = now
                );
                buffer.push_str(&row);
                buffer.push('\n');
            }

            let contracts_len = addresses.len();
            let copy = CopyStatement::new(
                "COPY contract_verification_info_v2(
                initial_contract_addr,
                bytecode_keccak256,
                bytecode_without_metadata_keccak256,
                verification_info,
                created_at,
                updated_at
            ) FROM STDIN (FORMAT CSV, NULL 'null', DELIMITER ',')",
            )
            .instrument("perform_verification_info_migration#copy")
            .with_arg("cursor", &cursor_str)
            .with_arg("contracts.len", &contracts_len)
            .start(&mut transaction)
            .await?;

            copy.send(buffer.as_bytes()).await?;

            offset += batch_size;
            cursor = addresses.last().unwrap().clone();
        }

        // Sanity check.
        tracing::info!("All the rows are migrated, verifying the migration");
        let row = sqlx::query!(
            r#"
            SELECT
                COUNT(*) AS count_equal,
                (SELECT COUNT(*) FROM contracts_verification_info) AS count_v1
            FROM
                contract_verification_info_v2 v2
            JOIN contracts_verification_info v1 ON initial_contract_addr = address
            WHERE v1.verification_info::text = v2.verification_info::text
            "#,
        )
        .instrument("is_verification_info_migration_performed")
        .fetch_one(&mut transaction)
        .await?;
        let (count_equal, count_v1) = (row.count_equal.unwrap(), row.count_v1.unwrap());
        if count_equal != count_v1 {
            anyhow::bail!(
                "Migration failed: v1 table has {count_v1} rows, but only {count_equal} matched after migration",
            );
        }

        tracing::info!("Migration is successful, committing the transaction");
        transaction.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use zksync_types::{
        bytecode::BytecodeHash,
        contract_verification::api::{CompilerVersions, SourceCodeData},
        tx::IncludedTxLocation,
        Execute, L1BatchNumber, L2BlockNumber, ProtocolVersion,
    };
    use zksync_vm_interface::{tracer::ValidationTraces, TransactionExecutionMetrics};

    use super::*;
    use crate::{
        tests::{create_l2_block_header, mock_l2_transaction},
        ConnectionPool, CoreDal,
    };

    #[tokio::test]
    async fn getting_contract_info_for_verification() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
        conn.blocks_dal()
            .insert_l2_block(&create_l2_block_header(0))
            .await
            .unwrap();

        // Add a transaction, its bytecode and the bytecode deployment event.
        let deployed_address = Address::repeat_byte(12);
        let mut tx = mock_l2_transaction();
        let bytecode = vec![1; 32];
        let bytecode_hash = BytecodeHash::for_bytecode(&bytecode).value();
        tx.execute = Execute::for_deploy(H256::zero(), bytecode.clone(), &[]);
        conn.transactions_dal()
            .insert_transaction_l2(
                &tx,
                TransactionExecutionMetrics::default(),
                ValidationTraces::default(),
            )
            .await
            .unwrap();
        conn.factory_deps_dal()
            .insert_factory_deps(
                L2BlockNumber(0),
                &HashMap::from([(bytecode_hash, bytecode.clone())]),
            )
            .await
            .unwrap();
        let location = IncludedTxLocation {
            tx_hash: tx.hash(),
            tx_index_in_l2_block: 0,
        };
        let deploy_event = VmEvent {
            location: (L1BatchNumber(0), 0),
            address: CONTRACT_DEPLOYER_ADDRESS,
            indexed_topics: vec![
                VmEvent::DEPLOY_EVENT_SIGNATURE,
                address_to_h256(&tx.initiator_account()),
                bytecode_hash,
                address_to_h256(&deployed_address),
            ],
            value: vec![],
        };
        conn.events_dal()
            .save_events(L2BlockNumber(0), &[(location, vec![&deploy_event])])
            .await
            .unwrap();

        let contract = conn
            .contract_verification_dal()
            .get_contract_info_for_verification(deployed_address)
            .await
            .unwrap()
            .expect("no info");
        assert_eq!(contract.bytecode_hash, bytecode_hash);
        assert_eq!(contract.bytecode, bytecode);
        assert_eq!(contract.contract_address, Some(CONTRACT_DEPLOYER_ADDRESS));
        assert_eq!(contract.calldata.unwrap(), tx.execute.calldata);
    }

    async fn test_working_with_verification_requests(zksolc: Option<&str>) {
        let request = VerificationIncomingRequest {
            contract_address: Address::repeat_byte(11),
            source_code_data: SourceCodeData::SolSingleFile("contract Test {}".to_owned()),
            contract_name: "Test".to_string(),
            compiler_versions: CompilerVersions::Solc {
                compiler_zksolc_version: zksolc.map(str::to_owned),
                compiler_solc_version: "0.8.27".to_owned(),
            },
            optimization_used: true,
            optimizer_mode: Some("z".to_owned()),
            constructor_arguments: web3::Bytes(b"test".to_vec()),
            is_system: false,
            force_evmla: true,
            evm_specific: Default::default(),
        };

        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        let id = conn
            .contract_verification_dal()
            .add_contract_verification_request(&request)
            .await
            .unwrap();

        let status = conn
            .contract_verification_dal()
            .get_verification_request_status(id)
            .await
            .unwrap()
            .expect("request not persisted");
        assert_eq!(status.status, "queued");

        let req = conn
            .contract_verification_dal()
            .get_next_queued_verification_request(Duration::from_secs(600))
            .await
            .unwrap()
            .expect("request not queued");
        assert_eq!(req.id, id);
        assert_eq!(req.req.contract_address, request.contract_address);
        assert_eq!(req.req.contract_name, request.contract_name);
        assert_eq!(req.req.compiler_versions, request.compiler_versions);
        assert_eq!(req.req.optimization_used, request.optimization_used);
        assert_eq!(req.req.optimizer_mode, request.optimizer_mode);
        assert_eq!(req.req.constructor_arguments, request.constructor_arguments);
        assert_eq!(req.req.is_system, request.is_system);
        assert_eq!(req.req.force_evmla, request.force_evmla);

        let maybe_req = conn
            .contract_verification_dal()
            .get_next_queued_verification_request(Duration::from_secs(600))
            .await
            .unwrap();
        assert!(maybe_req.is_none());
    }

    #[tokio::test]
    async fn test_update_verification_request_compiler_versions() {
        let request = VerificationIncomingRequest {
            contract_address: Address::repeat_byte(11),
            source_code_data: SourceCodeData::SolSingleFile("contract Test {}".to_owned()),
            contract_name: "Test".to_string(),
            compiler_versions: CompilerVersions::Solc {
                compiler_zksolc_version: Some("v1.5.10".to_owned()),
                compiler_solc_version: "0.8.20".to_owned(),
            },
            optimization_used: true,
            optimizer_mode: Some("z".to_owned()),
            constructor_arguments: web3::Bytes(b"test".to_vec()),
            is_system: false,
            force_evmla: true,
            evm_specific: Default::default(),
        };

        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        let id = conn
            .contract_verification_dal()
            .add_contract_verification_request(&request)
            .await
            .unwrap();

        let updated_compiler_version = "0.8.27";
        let updated_zksolc_version = Some("1.5.7");

        conn.contract_verification_dal()
            .update_verification_request_compiler_versions(
                id,
                updated_compiler_version,
                updated_zksolc_version,
            )
            .await
            .expect("request compiler versions not updated");

        let req = conn
            .contract_verification_dal()
            .get_next_queued_verification_request(Duration::from_secs(600))
            .await
            .unwrap()
            .expect("request not queued");
        assert_eq!(req.id, id);
        assert_eq!(
            req.req.compiler_versions.compiler_version(),
            updated_compiler_version
        );
        assert_eq!(
            req.req.compiler_versions.zk_compiler_version(),
            updated_zksolc_version
        );
    }

    #[tokio::test]
    async fn working_with_verification_requests() {
        test_working_with_verification_requests(None).await;
        test_working_with_verification_requests(Some("1.5.7")).await;
    }
}

#![doc = include_str!("../doc/ContractVerificationDal.md")]
use std::{
    fmt::{Display, Formatter},
    time::Duration,
};

use anyhow::Context as _;
use sqlx::postgres::types::PgInterval;
use zksync_db_connection::connection::Connection;
use zksync_types::{
    contract_verification_api::{
        DeployContractCalldata, VerificationIncomingRequest, VerificationInfo, VerificationRequest,
        VerificationRequestStatus,
    },
    Address, CONTRACT_DEPLOYER_ADDRESS,
};
use zksync_utils::address_to_h256;
use zksync_vm_interface::VmEvent;

use crate::{models::storage_verification_request::StorageVerificationRequest, Core};

#[derive(Debug)]
pub struct ContractVerificationDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

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

impl ContractVerificationDal<'_, '_> {
    pub async fn get_count_of_queued_verification_requests(&mut self) -> sqlx::Result<usize> {
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
        .fetch_one(self.storage.conn())
        .await
        .map(|row| row.count as usize)
    }

    pub async fn add_contract_verification_request(
        &mut self,
        query: VerificationIncomingRequest,
    ) -> sqlx::Result<usize> {
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
                    status,
                    created_at,
                    updated_at
                )
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'queued', NOW(), NOW())
            RETURNING
                id
            "#,
            query.contract_address.as_bytes(),
            // Serialization should always succeed.
            serde_json::to_string(&query.source_code_data).unwrap(),
            query.contract_name,
            query.compiler_versions.zk_compiler_version(),
            query.compiler_versions.compiler_version(),
            query.optimization_used,
            query.optimizer_mode,
            query.constructor_arguments.0,
            query.is_system,
            query.force_evmla,
        )
        .fetch_one(self.storage.conn())
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
    ) -> sqlx::Result<Option<VerificationRequest>> {
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
                force_evmla
            "#,
            &processing_timeout
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(Into::into);
        Ok(result)
    }

    /// Updates the verification request status and inserts the verification info upon successful verification.
    pub async fn save_verification_info(
        &mut self,
        verification_info: VerificationInfo,
    ) -> anyhow::Result<()> {
        let mut transaction = self
            .storage
            .start_transaction()
            .await
            .context("start_transaction()")?;

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
        .execute(transaction.conn())
        .await?;

        let address = verification_info.request.req.contract_address;
        // Serialization should always succeed.
        let verification_info_json = serde_json::to_value(verification_info)
            .expect("Failed to serialize verification info into serde_json");
        sqlx::query!(
            r#"
            INSERT INTO
                contracts_verification_info (address, verification_info)
            VALUES
                ($1, $2)
            ON CONFLICT (address) DO
            UPDATE
            SET
                verification_info = $2
            "#,
            address.as_bytes(),
            &verification_info_json
        )
        .execute(transaction.conn())
        .await?;

        transaction.commit().await.context("commit()")?;
        Ok(())
    }

    pub async fn save_verification_error(
        &mut self,
        id: usize,
        error: String,
        compilation_errors: serde_json::Value,
        panic_message: Option<String>,
    ) -> sqlx::Result<()> {
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
            error.as_str(),
            &compilation_errors,
            panic_message
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn get_verification_request_status(
        &mut self,
        id: usize,
    ) -> anyhow::Result<Option<VerificationRequestStatus>> {
        let Some(row) = sqlx::query!(
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
        .fetch_optional(self.storage.conn())
        .await?
        else {
            return Ok(None);
        };

        let mut compilation_errors = vec![];
        if let Some(errors) = row.compilation_errors {
            for value in errors.as_array().context("expected an array")? {
                compilation_errors.push(value.as_str().context("expected string")?.to_string());
            }
        }
        Ok(Some(VerificationRequestStatus {
            status: row.status,
            error: row.error,
            compilation_errors: if compilation_errors.is_empty() {
                None
            } else {
                Some(compilation_errors)
            },
        }))
    }

    /// Returns bytecode and calldata from the contract and the transaction that created it.
    pub async fn get_contract_info_for_verification(
        &mut self,
        address: Address,
    ) -> anyhow::Result<Option<(Vec<u8>, DeployContractCalldata)>> {
        let address_h256 = address_to_h256(&address);
        let deploy_signature = VmEvent::deploy_signature();

        let Some(row) = sqlx::query!(
            r#"
            SELECT
                factory_deps.bytecode,
                transactions.data AS "data?",
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
            deploy_signature.as_bytes(),
            address_h256.as_bytes(),
        )
        .fetch_optional(self.storage.conn())
        .await?
        else {
            return Ok(None);
        };
        let calldata = match row.contract_address {
            Some(contract_address) if contract_address == CONTRACT_DEPLOYER_ADDRESS.0.to_vec() => {
                // `row.contract_address` and `row.data` are either both `None` or both `Some(_)`.
                // In this arm it's checked that `row.contract_address` is `Some(_)`, so it's safe to unwrap `row.data`.
                let data: serde_json::Value = row.data.context("data missing")?;
                let calldata_str: String = serde_json::from_value(
                    data.get("calldata").context("calldata missing")?.clone(),
                )
                .context("failed parsing calldata")?;
                let calldata = hex::decode(&calldata_str[2..]).context("invalid calldata")?;
                DeployContractCalldata::Deploy(calldata)
            }
            _ => DeployContractCalldata::Ignore,
        };
        Ok(Some((row.bytecode, calldata)))
    }

    /// Returns true if the contract has a stored contracts_verification_info.
    pub async fn is_contract_verified(&mut self, address: Address) -> sqlx::Result<bool> {
        let count = sqlx::query!(
            r#"
            SELECT
                COUNT(*) AS "count!"
            FROM
                contracts_verification_info
            WHERE
                address = $1
            "#,
            address.as_bytes()
        )
        .fetch_one(self.storage.conn())
        .await?
        .count;
        Ok(count > 0)
    }

    async fn get_compiler_versions(&mut self, compiler: Compiler) -> sqlx::Result<Vec<String>> {
        let compiler = format!("{compiler}");
        let versions: Vec<_> = sqlx::query!(
            r#"
            SELECT
                VERSION
            FROM
                compiler_versions
            WHERE
                compiler = $1
            ORDER BY
                VERSION
            "#,
            &compiler
        )
        .fetch_all(self.storage.conn())
        .await?
        .into_iter()
        .map(|row| row.version)
        .collect();
        Ok(versions)
    }

    pub async fn get_zksolc_versions(&mut self) -> sqlx::Result<Vec<String>> {
        self.get_compiler_versions(Compiler::ZkSolc).await
    }

    pub async fn get_solc_versions(&mut self) -> sqlx::Result<Vec<String>> {
        self.get_compiler_versions(Compiler::Solc).await
    }

    pub async fn get_zkvyper_versions(&mut self) -> sqlx::Result<Vec<String>> {
        self.get_compiler_versions(Compiler::ZkVyper).await
    }

    pub async fn get_vyper_versions(&mut self) -> sqlx::Result<Vec<String>> {
        self.get_compiler_versions(Compiler::Vyper).await
    }

    async fn set_compiler_versions(
        &mut self,
        compiler: Compiler,
        versions: Vec<String>,
    ) -> anyhow::Result<()> {
        let mut transaction = self
            .storage
            .start_transaction()
            .await
            .context("start_transaction")?;
        let compiler = format!("{compiler}");

        sqlx::query!(
            r#"
            DELETE FROM compiler_versions
            WHERE
                compiler = $1
            "#,
            &compiler
        )
        .execute(transaction.conn())
        .await?;

        sqlx::query!(
            r#"
            INSERT INTO
                compiler_versions (VERSION, compiler, created_at, updated_at)
            SELECT
                u.version,
                $2,
                NOW(),
                NOW()
            FROM
                UNNEST($1::TEXT[]) AS u (VERSION)
            ON CONFLICT (VERSION, compiler) DO NOTHING
            "#,
            &versions,
            &compiler,
        )
        .execute(transaction.conn())
        .await?;

        transaction.commit().await.context("commit()")?;
        Ok(())
    }

    pub async fn set_zksolc_versions(&mut self, versions: Vec<String>) -> anyhow::Result<()> {
        self.set_compiler_versions(Compiler::ZkSolc, versions).await
    }

    pub async fn set_solc_versions(&mut self, versions: Vec<String>) -> anyhow::Result<()> {
        self.set_compiler_versions(Compiler::Solc, versions).await
    }

    pub async fn set_zkvyper_versions(&mut self, versions: Vec<String>) -> anyhow::Result<()> {
        self.set_compiler_versions(Compiler::ZkVyper, versions)
            .await
    }

    pub async fn set_vyper_versions(&mut self, versions: Vec<String>) -> anyhow::Result<()> {
        self.set_compiler_versions(Compiler::Vyper, versions).await
    }

    pub async fn get_all_successful_requests(&mut self) -> sqlx::Result<Vec<VerificationRequest>> {
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
                force_evmla
            FROM
                contract_verification_requests
            WHERE
                status = 'successful'
            ORDER BY
                id
            "#,
        )
        .fetch_all(self.storage.conn())
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
        let Some(row) = sqlx::query!(
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
        .fetch_optional(self.storage.conn())
        .await?
        else {
            return Ok(None);
        };
        let Some(info) = row.verification_info else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_value(info).context("invalid info")?))
    }
}

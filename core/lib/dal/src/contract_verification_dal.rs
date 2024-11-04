#![doc = include_str!("../doc/ContractVerificationDal.md")]

use std::{
    fmt::{Display, Formatter},
    time::Duration,
};

use sqlx::postgres::types::PgInterval;
use zksync_db_connection::{error::SqlxContext, instrument::InstrumentExt};
use zksync_types::{
    contract_verification_api::{
        VerificationIncomingRequest, VerificationInfo, VerificationRequest,
        VerificationRequestStatus,
    },
    web3, Address, CONTRACT_DEPLOYER_ADDRESS, H256,
};
use zksync_utils::address_to_h256;
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

    pub async fn add_contract_verification_request(
        &mut self,
        query: VerificationIncomingRequest,
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
            force_evmla
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

    /// Returns true if the contract has a stored contracts_verification_info.
    pub async fn is_contract_verified(&mut self, address: Address) -> DalResult<bool> {
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
        .instrument("is_contract_verified")
        .with_arg("address", &address)
        .fetch_one(self.storage)
        .await?
        .count;
        Ok(count > 0)
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
        versions: Vec<String>,
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
            &versions,
            &compiler,
        )
        .instrument("set_compiler_versions#insert")
        .with_arg("compiler", &compiler)
        .with_arg("versions.len", &versions.len())
        .execute(&mut transaction)
        .await?;

        transaction.commit().await
    }

    pub async fn set_zksolc_versions(&mut self, versions: Vec<String>) -> DalResult<()> {
        self.set_compiler_versions(Compiler::ZkSolc, versions).await
    }

    pub async fn set_solc_versions(&mut self, versions: Vec<String>) -> DalResult<()> {
        self.set_compiler_versions(Compiler::Solc, versions).await
    }

    pub async fn set_zkvyper_versions(&mut self, versions: Vec<String>) -> DalResult<()> {
        self.set_compiler_versions(Compiler::ZkVyper, versions)
            .await
    }

    pub async fn set_vyper_versions(&mut self, versions: Vec<String>) -> DalResult<()> {
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
                force_evmla
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
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use zksync_types::{
        tx::IncludedTxLocation, Execute, L1BatchNumber, L2BlockNumber, ProtocolVersion,
    };
    use zksync_utils::bytecode::hash_bytecode;
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
        let bytecode_hash = hash_bytecode(&bytecode);
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
            tx_initiator_address: tx.initiator_account(),
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
}

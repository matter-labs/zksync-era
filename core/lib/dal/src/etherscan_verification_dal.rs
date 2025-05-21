use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::postgres::types::PgInterval;
use zksync_db_connection::instrument::InstrumentExt;
use zksync_types::contract_verification::{
    api::VerificationRequest, etherscan::EtherscanVerification,
};

use crate::{
    models::storage_verification_request::StorageEtherscanVerificationRequest, Connection, Core,
    DalResult,
};

#[derive(Debug)]
pub struct EtherscanVerificationDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl EtherscanVerificationDal<'_, '_> {
    /// Inserts a new etherscan verification request which is linked to the original contract verification request by
    /// its ID.
    pub async fn add_verification_request(
        &mut self,
        contract_verification_request_id: usize,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO
            etherscan_verification_requests (
                contract_verification_request_id,
                status,
                created_at,
                updated_at
            )
            VALUES
            ($1, 'queued', NOW(), NOW())
            "#,
            contract_verification_request_id as i64,
        )
        .instrument("add_verification_request")
        .with_arg("id", &contract_verification_request_id)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    /// Returns the next verification request that is to be sent to Etherscan. Selects the request id from
    /// etherscan_verification_requests and then returns the actual request from joined contract_verification_requests.
    /// Handles the situation where processing of some request can be interrupted (panic, pod restart, etc..),
    /// `processing_timeout` parameter is used to avoid stuck requests.
    pub async fn get_next_queued_verification_request(
        &mut self,
        processing_timeout: Duration,
    ) -> DalResult<Option<(VerificationRequest, EtherscanVerification)>> {
        let processing_timeout = PgInterval {
            months: 0,
            days: 0,
            microseconds: processing_timeout.as_micros() as i64,
        };
        let result = sqlx::query_as!(
            StorageEtherscanVerificationRequest,
            r#"
            UPDATE etherscan_verification_requests evr
            SET
                status = 'in_progress',
                updated_at = NOW(),
                processing_started_at = NOW()
            FROM contract_verification_requests cvr
            WHERE
                evr.contract_verification_request_id = (
                    SELECT contract_verification_request_id
                    FROM etherscan_verification_requests
                    WHERE
                        (
                            (status = 'queued' AND (retry_at IS NULL OR retry_at < NOW()))
                            OR (
                                status = 'in_progress'
                                AND processing_started_at < NOW() - $1::INTERVAL
                            )
                        )
                    ORDER BY created_at
                    LIMIT 1
                    FOR UPDATE SKIP LOCKED
                )
                AND evr.contract_verification_request_id = cvr.id
            RETURNING
            cvr.id,
            cvr.contract_address,
            cvr.source_code,
            cvr.contract_name,
            cvr.zk_compiler_version,
            cvr.compiler_version,
            cvr.optimization_used,
            cvr.optimizer_mode,
            cvr.constructor_arguments,
            cvr.is_system,
            cvr.force_evmla,
            cvr.evm_specific,
            evr.etherscan_verification_id,
            evr.attempts,
            evr.retry_at
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

    pub async fn save_etherscan_verification_id(
        &mut self,
        request_id: usize,
        etherscan_verification_id: &str,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE etherscan_verification_requests
            SET
                etherscan_verification_id = $2,
                updated_at = NOW()
            WHERE
                contract_verification_request_id = $1
            "#,
            request_id as i64,
            etherscan_verification_id,
        )
        .instrument("save_etherscan_verification_id")
        .with_arg("request_id", &request_id)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn save_verification_success(&mut self, request_id: usize) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE etherscan_verification_requests
            SET
                attempts = attempts + 1,
                status = 'successful',
                updated_at = NOW()
            WHERE
                contract_verification_request_id = $1
            "#,
            request_id as i64,
        )
        .instrument("save_verification_success")
        .with_arg("request_id", &request_id)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn save_verification_failure(
        &mut self,
        request_id: usize,
        error: &str,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE etherscan_verification_requests
            SET
                attempts = attempts + 1,
                status = 'failed',
                updated_at = NOW(),
                error = $2
            WHERE
                contract_verification_request_id = $1
            "#,
            request_id as i64,
            error,
        )
        .instrument("save_verification_failure")
        .with_arg("request_id", &request_id)
        .with_arg("error", &error)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn save_for_retry(
        &mut self,
        request_id: usize,
        attempts: i32,
        retry_at: DateTime<Utc>,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE etherscan_verification_requests
            SET
                status = 'queued',
                attempts = $2,
                updated_at = NOW(),
                retry_at = $3,
                processing_started_at = NULL
            WHERE
                contract_verification_request_id = $1
            "#,
            request_id as i64,
            attempts as i32,
            retry_at.naive_utc(),
        )
        .instrument("save_for_retry")
        .with_arg("request_id", &request_id)
        .with_arg("attempts", &attempts)
        .execute(self.storage)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{
        contract_verification::api::{
            CompilerVersions, SourceCodeData, VerificationIncomingRequest,
        },
        web3, Address,
    };

    use super::*;
    use crate::{ConnectionPool, CoreDal};

    fn get_default_verification_request(zksolc: Option<&str>) -> VerificationIncomingRequest {
        VerificationIncomingRequest {
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
        }
    }

    async fn test_verification_requests(zksolc: Option<&str>) {
        let request = get_default_verification_request(zksolc);

        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        let id = conn
            .contract_verification_dal()
            .add_contract_verification_request(&request)
            .await
            .unwrap();

        conn.etherscan_verification_dal()
            .add_verification_request(id)
            .await
            .unwrap();

        let (req, verification) = conn
            .etherscan_verification_dal()
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
        assert_eq!(verification.attempts, 0);
        assert_eq!(verification.etherscan_verification_id, None);
        assert_eq!(verification.retry_at, None);

        let maybe_req = conn
            .etherscan_verification_dal()
            .get_next_queued_verification_request(Duration::from_secs(600))
            .await
            .unwrap();
        assert!(maybe_req.is_none());
    }

    #[tokio::test]
    async fn test_working_with_verification_requests() {
        test_verification_requests(None).await;
        test_verification_requests(Some("1.5.7")).await;
    }

    #[tokio::test]
    async fn test_updating_verification_request_successful_status() {
        let request = get_default_verification_request(Some("1.5.7"));

        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        let id = conn
            .contract_verification_dal()
            .add_contract_verification_request(&request)
            .await
            .unwrap();

        conn.etherscan_verification_dal()
            .add_verification_request(id)
            .await
            .unwrap();

        conn.etherscan_verification_dal()
            .save_verification_success(id)
            .await
            .unwrap();

        let maybe_req = conn
            .etherscan_verification_dal()
            .get_next_queued_verification_request(Duration::from_secs(600))
            .await
            .unwrap();
        assert!(maybe_req.is_none());
    }

    #[tokio::test]
    async fn test_updating_verification_request_failed_status() {
        let request = get_default_verification_request(Some("1.5.7"));

        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        let id = conn
            .contract_verification_dal()
            .add_contract_verification_request(&request)
            .await
            .unwrap();

        conn.etherscan_verification_dal()
            .add_verification_request(id)
            .await
            .unwrap();

        conn.etherscan_verification_dal()
            .save_verification_failure(id, "Error processing the request")
            .await
            .unwrap();

        let maybe_req = conn
            .etherscan_verification_dal()
            .get_next_queued_verification_request(Duration::from_secs(600))
            .await
            .unwrap();
        assert!(maybe_req.is_none());
    }

    #[tokio::test]
    async fn test_updating_verification_request_etherscan_verification_id() {
        let request = get_default_verification_request(Some("1.5.7"));
        let etherscan_verification_id = "etherscan_verification_id";

        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        let id = conn
            .contract_verification_dal()
            .add_contract_verification_request(&request)
            .await
            .unwrap();

        conn.etherscan_verification_dal()
            .add_verification_request(id)
            .await
            .unwrap();

        conn.etherscan_verification_dal()
            .save_etherscan_verification_id(id, etherscan_verification_id)
            .await
            .unwrap();

        let (_, verification) = conn
            .etherscan_verification_dal()
            .get_next_queued_verification_request(Duration::from_secs(600))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            verification.etherscan_verification_id,
            Some(etherscan_verification_id.to_string())
        );
    }

    #[tokio::test]
    async fn test_saving_request_for_retry() {
        let request = get_default_verification_request(Some("1.5.7"));
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        let id = conn
            .contract_verification_dal()
            .add_contract_verification_request(&request)
            .await
            .unwrap();

        conn.etherscan_verification_dal()
            .add_verification_request(id)
            .await
            .unwrap();

        let retry_at = Utc::now();
        let attempts_number: i32 = 10;
        conn.etherscan_verification_dal()
            .save_for_retry(id, 10, retry_at)
            .await
            .unwrap();

        let (_, verification) = conn
            .etherscan_verification_dal()
            .get_next_queued_verification_request(Duration::from_secs(600))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(verification.attempts, attempts_number);
        assert_eq!(
            verification.retry_at.unwrap().timestamp(),
            retry_at.timestamp()
        );
    }
}

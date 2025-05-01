use chrono::NaiveDateTime;
use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};

use crate::Core;

#[derive(Debug)]
pub struct TeeDcapCollateralDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}
impl TeeDcapCollateralDal<'_, '_> {
    pub async fn cert_is_current(
        &mut self,
        serial_number: &[u8],
        issuer: &str,
        sha256: &[u8],
    ) -> DalResult<bool> {
        Ok(sqlx::query_scalar!(
            r#"
                SELECT count(serial_number) FROM tee_dcap_collateral_certs
                WHERE serial_number = $1 
                AND issuer = $2
                AND sha256 = $3
                AND transaction_timestamp() < not_after
            "#,
            serial_number,
            issuer,
            sha256
        )
        .instrument("tee_dcap_collateral_cert_is_current")
        .with_arg("serial_number", &serial_number)
        .with_arg("issuer", &issuer)
        .fetch_one(self.storage)
        .await?
        .is_some_and(|n| n > 0))
    }

    pub async fn cert_updated(
        &mut self,
        serial_number: &[u8],
        issuer: &str,
        sha256: &[u8],
        not_after: NaiveDateTime,
    ) -> DalResult<()> {
        let query = sqlx::query!(
            r#"
            INSERT INTO tee_dcap_collateral_certs
            (serial_number, issuer, sha256, not_after, updated)
            VALUES ($1, $2, $3, $4, transaction_timestamp())
            ON CONFLICT (serial_number, issuer)
            DO UPDATE SET
            sha256 = $3,
            not_after = $4,
            updated = transaction_timestamp()
            "#,
            serial_number,
            issuer,
            sha256,
            not_after
        );
        query
            .instrument("tee_dcap_collateral_cert_updated")
            .with_arg("serial_number", &serial_number)
            .with_arg("issuer", &issuer)
            .execute(self.storage)
            .await?;
        Ok(())
    }

    pub async fn root_crl_is_current(&mut self, sha256: &[u8]) -> DalResult<bool> {
        Ok(sqlx::query_scalar!(
            r#"
                SELECT count(kind) FROM tee_dcap_collateral
                WHERE kind = 'root_crl'
                AND sha256 = $1
                AND transaction_timestamp() < not_after
            "#,
            sha256
        )
        .instrument("tee_dcap_collateral_root_crl_is_current")
        .fetch_one(self.storage)
        .await?
        .is_some_and(|n| n > 0))
    }

    pub async fn root_crl_updated(
        &mut self,
        sha256: &[u8],
        not_after: NaiveDateTime,
    ) -> DalResult<()> {
        let query = sqlx::query!(
            r#"
            INSERT INTO tee_dcap_collateral (kind, sha256, not_after, updated)
            VALUES ('root_crl', $1, $2, transaction_timestamp())
            ON CONFLICT (kind)
            DO UPDATE SET
            sha256 = $1,
            not_after = $2,
            updated = transaction_timestamp()
            "#,
            sha256,
            not_after
        );
        query
            .instrument("tee_dcap_collateral_root_crl_updated")
            .execute(self.storage)
            .await?;
        Ok(())
    }

    pub async fn pck_crl_is_current(&mut self, sha256: &[u8]) -> DalResult<bool> {
        Ok(sqlx::query_scalar!(
            r#"
                SELECT count(kind) FROM tee_dcap_collateral
                WHERE kind = 'pck_crl'
                AND sha256 = $1
                AND transaction_timestamp() < not_after
            "#,
            sha256
        )
        .instrument("tee_dcap_collateral_pck_crl_is_current")
        .fetch_one(self.storage)
        .await?
        .is_some_and(|n| n > 0))
    }

    pub async fn pck_crl_updated(
        &mut self,
        sha256: &[u8],
        not_after: NaiveDateTime,
    ) -> DalResult<()> {
        let query = sqlx::query!(
            r#"
            INSERT INTO tee_dcap_collateral (kind, sha256, not_after, updated)
            VALUES ('pck_crl', $1, $2, transaction_timestamp())
            ON CONFLICT (kind)
            DO UPDATE SET
            sha256 = $1,
            not_after = $2,
            updated = transaction_timestamp()
            "#,
            sha256,
            not_after
        );
        query
            .instrument("tee_dcap_collateral_pck_crl_updated")
            .execute(self.storage)
            .await?;
        Ok(())
    }

    pub async fn tcb_info_json_is_current(&mut self, sha256: &[u8]) -> DalResult<bool> {
        Ok(sqlx::query_scalar!(
            r#"
                SELECT count(kind) FROM tee_dcap_collateral
                WHERE kind = 'tcb_info_json'
                AND sha256 = $1
                AND transaction_timestamp() < not_after
            "#,
            sha256
        )
        .instrument("tee_dcap_collateral_tcb_info_json_is_current")
        .fetch_one(self.storage)
        .await?
        .is_some_and(|n| n > 0))
    }

    pub async fn tcb_info_json_updated(
        &mut self,
        sha256: &[u8],
        not_after: NaiveDateTime,
    ) -> DalResult<()> {
        let query = sqlx::query!(
            r#"
            INSERT INTO tee_dcap_collateral (kind, sha256, not_after, updated)
            VALUES ('tcb_info_json', $1, $2, transaction_timestamp())
            ON CONFLICT (kind)
            DO UPDATE SET
            sha256 = $1,
            not_after = $2,
            updated = transaction_timestamp()
            "#,
            sha256,
            not_after
        );
        query
            .instrument("tee_dcap_collateral_tcb_info_json_updated")
            .execute(self.storage)
            .await?;
        Ok(())
    }

    pub async fn qe_identity_json_is_current(&mut self, sha256: &[u8]) -> DalResult<bool> {
        Ok(sqlx::query_scalar!(
            r#"
                SELECT count(kind) FROM tee_dcap_collateral
                WHERE kind = 'qe_identity_json'
                AND sha256 = $1
                AND transaction_timestamp() < not_after
            "#,
            sha256
        )
        .instrument("tee_dcap_collateral_qe_identity_json_is_current")
        .fetch_one(self.storage)
        .await?
        .is_some_and(|n| n > 0))
    }

    pub async fn qe_identity_json_updated(
        &mut self,
        sha256: &[u8],
        not_after: NaiveDateTime,
    ) -> DalResult<()> {
        let query = sqlx::query!(
            r#"
            INSERT INTO tee_dcap_collateral (kind, sha256, not_after, updated)
            VALUES ('qe_identity_json', $1, $2, transaction_timestamp())
            ON CONFLICT (kind)
            DO UPDATE SET
            sha256 = $1,
            not_after = $2,
            updated = transaction_timestamp()
            "#,
            sha256,
            not_after
        );
        query
            .instrument("tee_dcap_collateral_qe_identity_json_updated")
            .execute(self.storage)
            .await?;
        Ok(())
    }
}

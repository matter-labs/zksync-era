use chrono::{DateTime, Utc};
use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};

use crate::Core;

/// The status of a specific piece of collateral that is stored on chain
pub enum TeeDcapCollateralInfo {
    /// The collateral provided matches what is present in the database and on chain
    Matches,
    /// The collateral provided does not match what is in the database, the receipient of this data
    /// has a responsibility to update the chain by the time specified.
    UpdateChainBy(DateTime<Utc>),
    /// The collateral provided does not match what is in the database, another agent has already
    /// been tasked with updating the chain by the time specified, at which point a retry may be
    /// necessary.
    PendingUpdateBy(DateTime<Utc>),
    /// No record of the collateral on the chain is present, it must be created
    RecordMissing,
}

#[derive(sqlx::Type, Debug, Copy, Clone)]
#[sqlx(type_name = "tee_dcap_collateral_kind", rename_all = "snake_case")]
pub enum TeeDcapCollateralKind {
    RootCrl,
    PckCrl,
    SgxTcbInfoJson,
    TdxTcbInfoJson,
    SgxQeIdentityJson,
    TdxQeIdentityJson,
}

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
    ) -> DalResult<TeeDcapCollateralInfo> {
        let mut tx = self.storage.start_transaction().await?;
        if let Some(record) = sqlx::query!(
            r#"
            SELECT
                sha256 = $3 AND not_after < transaction_timestamp() AS "sha_matches",
                CASE
                    WHEN update_guard_expires IS NOT NULL
                        THEN update_guard_expires < transaction_timestamp()
                    ELSE
                        NULL
                END AS "guard_expired",
                update_guard_expires
            FROM tee_dcap_collateral_certs
            WHERE
                serial_number = $1
                AND issuer = $2
            "#,
            serial_number,
            issuer,
            sha256,
        )
        .instrument("tee_dcap_collateral_cert_is_current")
        .with_arg("serial_number", &serial_number)
        .with_arg("issuer", &issuer)
        .fetch_optional(&mut tx)
        .await?
        {
            match (record.guard_expired, record.sha_matches) {
                (None, Some(true)) => {
                    // No record of the guard exists, and the sha matches
                    Ok(TeeDcapCollateralInfo::Matches)
                }
                (Some(true), _) | (None, Some(false)) => {
                    // The guard either does not exist, or has expired due to the previous caller
                    // responsible for updating it taking too long to do so. In either case, the
                    // chain needs to be provided the new collateral and the database updated, and
                    // it is the callers responsibility to do this.
                    let guard_expires = sqlx::query_scalar!(
                        r#"
                            UPDATE tee_dcap_collateral_certs
                            SET update_guard_expires = transaction_timestamp() + (
                                SELECT delay FROM chain_update_delay WHERE id = 1
                            )
                            WHERE serial_number = $1
                            AND issuer = $2
                            RETURNING update_guard_expires
                        "#,
                        serial_number,
                        issuer
                    )
                    .instrument("tee_dcap_collateral_cert_lock")
                    .with_arg("serial_number", &serial_number)
                    .with_arg("issuer", &issuer)
                    .fetch_one(&mut tx)
                    .await?
                    .unwrap(); // value is not null as it was just set
                    Ok(TeeDcapCollateralInfo::UpdateChainBy(guard_expires))
                }
                (Some(false), _) => {
                    // The guard has not yet expired, a previous caller has been tasked with
                    // updating the information on the chain and reflecting this in the database
                    // upon completion.
                    Ok(TeeDcapCollateralInfo::PendingUpdateBy(
                        record.update_guard_expires.unwrap(),
                    ))
                }
                (_, None) => {
                    // Case cannot occur as a NOT NULL field cannot produce a null equality check
                    // unless checked with null, and there is no possibility to pass in a nullish
                    // sha256.
                    unreachable!()
                }
            }
        } else {
            // No record of cert, create record and inform caller they need to put this record on the chain.
            Ok(TeeDcapCollateralInfo::RecordMissing)
        }
    }

    pub async fn cert_updated(
        &mut self,
        serial_number: &[u8],
        issuer: &str,
        sha256: &[u8],
        not_after: DateTime<Utc>,
    ) -> DalResult<()> {
        let query = sqlx::query!(
            r#"
            INSERT INTO tee_dcap_collateral_certs
            (serial_number, issuer, sha256, not_after, updated, update_guard_expires)
            VALUES ($1, $2, $3, $4, transaction_timestamp(), NULL)
            ON CONFLICT (serial_number, issuer)
            DO UPDATE SET
            sha256 = $3,
            not_after = $4,
            updated = transaction_timestamp(),
            update_guard_expires = NULL
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

    pub async fn field_is_current(
        &mut self,
        kind: TeeDcapCollateralKind,
        sha256: &[u8],
    ) -> DalResult<TeeDcapCollateralInfo> {
        let mut tx = self.storage.start_transaction().await?;
        if let Some(record) = sqlx::query!(
            r#"
            SELECT
                sha256 = $2 AND not_after < transaction_timestamp() AS "sha_matches",
                CASE
                    WHEN update_guard_expires IS NOT NULL
                        THEN update_guard_expires < transaction_timestamp()
                    ELSE
                        NULL
                END AS "guard_expired",
                update_guard_expires
            FROM tee_dcap_collateral
            WHERE kind = $1
            "#,
            kind as _,
            sha256,
        )
        .instrument("tee_dcap_collateral_field_is_current")
        .with_arg("field", &kind)
        .fetch_optional(&mut tx)
        .await?
        {
            match (record.guard_expired, record.sha_matches) {
                (None, Some(true)) => {
                    // No record of the guard exists, and the sha matches
                    Ok(TeeDcapCollateralInfo::Matches)
                }
                (Some(true), _) | (None, Some(false)) => {
                    // The guard either does not exist, or has expired due to the previous caller
                    // responsible for updating it taking too long to do so. In either case, the
                    // chain needs to be provided the new collateral and the database updated, and
                    // it is the callers responsibility to do this.
                    let guard_expires = sqlx::query_scalar!(
                        r#"
                            UPDATE tee_dcap_collateral
                            SET update_guard_expires = transaction_timestamp() + (
                                SELECT delay FROM chain_update_delay WHERE id = 1
                            )
                            WHERE kind = $1
                            RETURNING update_guard_expires
                        "#,
                        kind as _
                    )
                    .instrument("tee_dcap_collateral_field_lock")
                    .with_arg("kind", &kind)
                    .fetch_one(&mut tx)
                    .await?
                    .unwrap(); // value is not null as it was just set
                    Ok(TeeDcapCollateralInfo::UpdateChainBy(guard_expires))
                }
                (Some(false), _) => {
                    // The guard has not yet expired, a previous caller has been tasked with
                    // updating the information on the chain and reflecting this in the database
                    // upon completion.
                    Ok(TeeDcapCollateralInfo::PendingUpdateBy(
                        record.update_guard_expires.unwrap(),
                    ))
                }
                (_, None) => {
                    // Case cannot occur as a NOT NULL field cannot produce a null equality check
                    // unless checked with null, and there is no possibility to pass in a nullish
                    // sha256.
                    unreachable!()
                }
            }
        } else {
            // No record of cert, create record and inform caller they need to put this record on the chain.
            Ok(TeeDcapCollateralInfo::RecordMissing)
        }
    }

    pub async fn field_updated(
        &mut self,
        kind: TeeDcapCollateralKind,
        sha256: &[u8],
        not_after: DateTime<Utc>,
    ) -> DalResult<()> {
        let query = sqlx::query!(
            r#"
            INSERT INTO tee_dcap_collateral
            (kind, sha256, not_after, updated, update_guard_expires)
            VALUES ($1, $2, $3, transaction_timestamp(), NULL)
            ON CONFLICT (kind)
            DO UPDATE SET
            sha256 = $2,
            not_after = $3,
            updated = transaction_timestamp(),
            update_guard_expires = NULL
            "#,
            kind as _,
            sha256,
            not_after
        );
        query
            .instrument("tee_dcap_collateral_updated")
            .with_arg("kind", &kind)
            .execute(self.storage)
            .await?;
        Ok(())
    }
}

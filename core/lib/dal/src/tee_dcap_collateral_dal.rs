use chrono::{DateTime, Utc};
use sqlx::postgres::types::PgInterval;
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
    /// The collateral has been updated, but the transaction to commit it to the chain is still
    /// pending confirmation.
    PendingEthSenderCompletion,
    /// No record of the collateral on the chain is present, it must be created
    RecordMissing,
}

#[derive(sqlx::Type, Debug, Copy, Clone)]
#[sqlx(type_name = "tee_dcap_collateral_kind", rename_all = "snake_case")]
pub enum TeeDcapCollateralKind {
    RootCa,
    RootCrl,
    PckCa,
    PckCrl,
    SgxQeIdentityJson,
    TdxQeIdentityJson,
}

#[derive(sqlx::Type, Debug, Copy, Clone)]
#[sqlx(
    type_name = "tee_dcap_collateral_tcb_info_json_kind",
    rename_all = "snake_case"
)]
pub enum TeeDcapCollateralTcbInfoJsonKind {
    SgxTcbInfoJson,
    TdxTcbInfoJson,
}

struct CurrentFieldValidator {
    sha_matches: bool,
    not_after: bool,
    update_guard_set: bool,
    update_guard_expires: DateTime<Utc>,
    update_guard_expired: bool,
    eth_tx_guard_set: bool,
    eth_tx_guard_confirmed: bool,
}

#[derive(Debug)]
pub struct TeeDcapCollateralDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}
impl TeeDcapCollateralDal<'_, '_> {
    pub async fn field_is_current(
        &mut self,
        kind: TeeDcapCollateralKind,
        sha256: &[u8],
        timeout: PgInterval,
    ) -> DalResult<TeeDcapCollateralInfo> {
        let mut tx = self.storage.start_transaction().await?;
        match sqlx::query_as!(
            CurrentFieldValidator,
            r#"
            SELECT
                c.sha256 = $2 AS "sha_matches!",
                transaction_timestamp() < c.not_after AS "not_after!",
                c.update_guard_set IS NULL AS "update_guard_set!",
                (
                    coalesce(c.update_guard_set, transaction_timestamp()) + $3
                ) AS "update_guard_expires!",
                (coalesce(c.update_guard_set, transaction_timestamp()) + $3)
                < transaction_timestamp() AS "update_guard_expired!",
                c.eth_tx_id IS NULL AS "eth_tx_guard_set!",
                e.confirmed_eth_tx_history_id IS NULL AS "eth_tx_guard_confirmed!"
            FROM
                tee_dcap_collateral c
            LEFT OUTER JOIN eth_txs e ON c.eth_tx_id = e.id
            WHERE c.kind = $1
            "#,
            kind as _,
            sha256,
            timeout
        )
        .instrument("tee_dcap_collateral_field_is_current")
        .with_arg("field", &kind)
        .fetch_optional(&mut tx)
        .await?
        {
            Some(CurrentFieldValidator {
                sha_matches: true,
                not_after: true,
                update_guard_set: false,
                eth_tx_guard_set: false,
                ..
            }) => {
                // All correct with no guards set
                Ok(TeeDcapCollateralInfo::Matches)
            }
            Some(CurrentFieldValidator {
                update_guard_set: true,
                update_guard_expired: false,
                update_guard_expires,
                ..
            }) => {
                // Update guard not yet expired
                Ok(TeeDcapCollateralInfo::PendingUpdateBy(update_guard_expires))
            }
            Some(CurrentFieldValidator {
                eth_tx_guard_set: true,
                eth_tx_guard_confirmed: false,
                ..
            }) => {
                // Waiting for eth sender
                Ok(TeeDcapCollateralInfo::PendingEthSenderCompletion)
            }
            Some(CurrentFieldValidator {
                sha_matches: true,
                not_after: true,
                eth_tx_guard_set: true,
                eth_tx_guard_confirmed: true,
                ..
            }) => {
                // Update confirmed and valid, remove guard
                sqlx::query!(
                    r#"
                    UPDATE tee_dcap_collateral
                    SET
                        update_guard_set = NULL,
                        eth_tx_id = NULL
                    WHERE
                        kind = $1
                    "#,
                    kind as _
                )
                .instrument("tee_dcap_collateral_field_is_current_remote_eth_tx_guard")
                .with_arg("field", &kind)
                .execute(&mut tx)
                .await?;
                tx.commit().await?;
                Ok(TeeDcapCollateralInfo::Matches)
            }
            Some(CurrentFieldValidator { .. }) => {
                // Some condition is incorrect, set guard
                // Update confirmed and valid, remove guard
                let record = sqlx::query!(
                    r#"
                    UPDATE tee_dcap_collateral
                    SET
                        update_guard_set = transaction_timestamp(),
                        eth_tx_id = NULL
                    WHERE
                        kind = $1
                    RETURNING
                    update_guard_set + $2 AS "update_guard_expires!"
                    "#,
                    kind as _,
                    timeout
                )
                .instrument("tee_dcap_collateral_field_is_current_remote_update_guard")
                .with_arg("field", &kind)
                .fetch_one(&mut tx)
                .await?;
                tx.commit().await?;
                Ok(TeeDcapCollateralInfo::PendingUpdateBy(
                    record.update_guard_expires,
                ))
            }
            None => {
                // Record missing from database, needs inserting
                let record = sqlx::query!(
                    r#"
                    INSERT INTO tee_dcap_collateral VALUES (
                        $1,
                        transaction_timestamp(),
                        $2,
                        transaction_timestamp(),
                        transaction_timestamp(),
                        NULL
                    ) RETURNING
                    transaction_timestamp() + $3 AS "update_guard_expires!"
                    "#,
                    kind as _,
                    &[],
                    timeout
                )
                .instrument("tee_dcap_collateral_field_is_current_remote_update_guard")
                .with_arg("field", &kind)
                .fetch_one(&mut tx)
                .await?;
                tx.commit().await?;
                Ok(TeeDcapCollateralInfo::PendingUpdateBy(
                    record.update_guard_expires,
                ))
            }
        }
    }

    pub async fn update_field(
        &mut self,
        kind: TeeDcapCollateralKind,
        sha256: &[u8],
        not_after: DateTime<Utc>,
        eth_tx_id: i32,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE tee_dcap_collateral
            SET
                update_guard_set = NULL,
                sha256 = $2,
                not_after = $3,
                eth_tx_id = $4,
                updated = transaction_timestamp()
            WHERE
                kind = $1
            "#,
            kind as _,
            sha256,
            not_after,
            eth_tx_id
        )
        .instrument("tee_dcap_collateral_update_field")
        .with_arg("field", &kind)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn tcb_info_is_current(
        &mut self,
        kind: TeeDcapCollateralTcbInfoJsonKind,
        fmspc: &[u8],
        sha256: &[u8],
        timeout: PgInterval,
    ) -> DalResult<TeeDcapCollateralInfo> {
        let mut tx = self.storage.start_transaction().await?;
        match sqlx::query_as!(
            CurrentFieldValidator,
            r#"
            SELECT
                c.sha256 = $2 AS "sha_matches!",
                transaction_timestamp() < c.not_after AS "not_after!",
                c.update_guard_set IS NULL AS "update_guard_set!",
                (
                    coalesce(c.update_guard_set, transaction_timestamp()) + $3
                ) AS "update_guard_expires!",
                (coalesce(c.update_guard_set, transaction_timestamp()) + $3)
                < transaction_timestamp() AS "update_guard_expired!",
                c.eth_tx_id IS NULL AS "eth_tx_guard_set!",
                e.confirmed_eth_tx_history_id IS NULL AS "eth_tx_guard_confirmed!"
            FROM
                tee_dcap_collateral_tcb_info_json c
            LEFT OUTER JOIN eth_txs e ON c.eth_tx_id = e.id
            WHERE c.kind = $1 AND c.fmspc = $4
            "#,
            kind as _,
            sha256,
            timeout,
            fmspc
        )
        .instrument("tee_dcap_collateral_tcb_info_is_current")
        .with_arg("field", &kind)
        .with_arg("fmspc", &fmspc)
        .fetch_optional(&mut tx)
        .await?
        {
            Some(CurrentFieldValidator {
                sha_matches: true,
                not_after: true,
                update_guard_set: false,
                eth_tx_guard_set: false,
                ..
            }) => {
                // All correct with no guards set
                Ok(TeeDcapCollateralInfo::Matches)
            }
            Some(CurrentFieldValidator {
                update_guard_set: true,
                update_guard_expired: false,
                update_guard_expires,
                ..
            }) => {
                // Update guard not yet expired
                Ok(TeeDcapCollateralInfo::PendingUpdateBy(update_guard_expires))
            }
            Some(CurrentFieldValidator {
                eth_tx_guard_set: true,
                eth_tx_guard_confirmed: false,
                ..
            }) => {
                // Waiting for eth sender
                Ok(TeeDcapCollateralInfo::PendingEthSenderCompletion)
            }
            Some(CurrentFieldValidator {
                sha_matches: true,
                not_after: true,
                eth_tx_guard_set: true,
                eth_tx_guard_confirmed: true,
                ..
            }) => {
                // Update confirmed and valid, remove guard
                sqlx::query!(
                    r#"
                    UPDATE tee_dcap_collateral_tcb_info_json
                    SET
                        update_guard_set = NULL,
                        eth_tx_id = NULL
                    WHERE
                        kind = $1 AND
                        fmspc = $2
                    "#,
                    kind as _,
                    fmspc
                )
                .instrument("tee_dcap_collateral_tcb_info_is_current_remote_eth_tx_guard")
                .with_arg("field", &kind)
                .with_arg("fmspc", &fmspc)
                .execute(&mut tx)
                .await?;
                tx.commit().await?;
                Ok(TeeDcapCollateralInfo::Matches)
            }
            Some(CurrentFieldValidator { .. }) => {
                // Some condition is incorrect, set guard
                // Update confirmed and valid, remove guard
                let record = sqlx::query!(
                    r#"
                    UPDATE tee_dcap_collateral_tcb_info_json
                    SET
                        update_guard_set = transaction_timestamp(),
                        eth_tx_id = NULL
                    WHERE
                        kind = $1 AND fmspc = $3
                    RETURNING
                    update_guard_set + $2 AS "update_guard_expires!"
                    "#,
                    kind as _,
                    timeout,
                    fmspc
                )
                .instrument("tee_dcap_collateral_tcb_info_is_current_remote_update_guard")
                .with_arg("field", &kind)
                .with_arg("fmspc", &fmspc)
                .fetch_one(&mut tx)
                .await?;
                tx.commit().await?;
                Ok(TeeDcapCollateralInfo::PendingUpdateBy(
                    record.update_guard_expires,
                ))
            }
            None => {
                // Record missing from database, needs inserting
                let record = sqlx::query!(
                    r#"
                    INSERT INTO tee_dcap_collateral_tcb_info_json VALUES (
                        $1,
                        $4,
                        transaction_timestamp(),
                        $2,
                        transaction_timestamp(),
                        transaction_timestamp(),
                        NULL
                    ) RETURNING
                    transaction_timestamp() + $3 AS "update_guard_expires!"
                    "#,
                    kind as _,
                    &[],
                    timeout,
                    fmspc
                )
                .instrument("tee_dcap_collateral_field_is_current_remote_update_guard")
                .with_arg("field", &kind)
                .fetch_one(&mut tx)
                .await?;
                tx.commit().await?;
                Ok(TeeDcapCollateralInfo::PendingUpdateBy(
                    record.update_guard_expires,
                ))
            }
        }
    }

    pub async fn update_tcb_info(
        &mut self,
        kind: TeeDcapCollateralKind,
        fmspc: &[u8],
        sha256: &[u8],
        not_after: DateTime<Utc>,
        eth_tx_id: i32,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE tee_dcap_collateral_tcb_info_json
            SET
                update_guard_set = NULL,
                sha256 = $2,
                not_after = $3,
                eth_tx_id = $4,
                updated = transaction_timestamp()
            WHERE
                kind = $1 AND fmspc = $5
            "#,
            kind as _,
            sha256,
            not_after,
            eth_tx_id,
            fmspc
        )
        .instrument("tee_dcap_collateral_update_field")
        .with_arg("field", &kind)
        .execute(self.storage)
        .await?;
        Ok(())
    }
}

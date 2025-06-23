use chrono::{DateTime, Utc};
use sqlx::postgres::types::PgInterval;
use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};

use crate::{Core, CoreDal};

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
}

#[derive(sqlx::Type, Debug, Copy, Clone)]
#[sqlx(type_name = "tee_dcap_collateral_kind", rename_all = "snake_case")]
pub enum TeeDcapCollateralKind {
    RootCa,
    RootCrl,
    PckCa,
    PckCrl,
    SignCa,
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
    calldata_set: bool,
    eth_tx_guard_set: bool,
    eth_tx_guard_confirmed: bool,
}

pub enum PendingCollateral {
    Field(PendingFieldCollateral),
    TcbInfo(PendingTcbInfoCollateral),
}
impl PendingCollateral {
    pub fn calldata(&self) -> &[u8] {
        let (PendingCollateral::Field(PendingFieldCollateral { calldata, .. })
        | PendingCollateral::TcbInfo(PendingTcbInfoCollateral { calldata, .. })) = self;
        calldata
    }
}

pub struct PendingFieldCollateral {
    pub kind: TeeDcapCollateralKind,
    pub calldata: Vec<u8>,
}

pub struct PendingTcbInfoCollateral {
    pub kind: TeeDcapCollateralTcbInfoJsonKind,
    pub fmspc: Vec<u8>,
    pub calldata: Vec<u8>,
}

pub enum ExpiringCollateral {
    Field(ExpiringFieldCollateral),
    TcbInfo(ExpiringTcbInfoCollateral),
}

pub struct ExpiringFieldCollateral {
    pub kind: TeeDcapCollateralKind,
    pub not_after: DateTime<Utc>,
}

pub struct ExpiringTcbInfoCollateral {
    pub kind: TeeDcapCollateralTcbInfoJsonKind,
    pub fmspc: Vec<u8>,
    pub not_after: DateTime<Utc>,
}

#[derive(Debug)]
pub struct TeeDcapCollateralDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}
impl TeeDcapCollateralDal<'_, '_> {
    pub const DEFAULT_TIMEOUT: PgInterval = PgInterval {
        months: 0,
        days: 1,
        microseconds: 0,
    };

    pub const DEFAULT_EXPIRES_WITHIN: PgInterval = PgInterval {
        months: 0,
        days: 7,
        microseconds: 0,
    };

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
                c.calldata IS NOT NULL AS "calldata_set!",
                c.eth_tx_id IS NOT NULL AS "eth_tx_guard_set!",
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
            Some(
                CurrentFieldValidator {
                    eth_tx_guard_set: true,
                    eth_tx_guard_confirmed: false,
                    ..
                }
                | CurrentFieldValidator {
                    calldata_set: true,
                    eth_tx_guard_set: false,
                    ..
                },
            ) => {
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
                c.calldata IS NOT NULL AS "calldata_set!",
                c.eth_tx_id IS NOT NULL AS "eth_tx_guard_set!",
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
            Some(
                CurrentFieldValidator {
                    eth_tx_guard_set: true,
                    eth_tx_guard_confirmed: false,
                    ..
                }
                | CurrentFieldValidator {
                    calldata_set: true,
                    eth_tx_guard_set: false,
                    ..
                },
            ) => {
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

    pub async fn get_pending_collateral_for_eth_tx(&mut self) -> DalResult<Vec<PendingCollateral>> {
        let mut tx = self.storage.start_transaction().await?;
        let out = tx
            .tee_dcap_collateral_dal()
            .internal_get_pending_collateral_for_eth_tx()
            .await?;
        tx.commit().await?;
        Ok(out)
    }

    async fn internal_get_pending_collateral_for_eth_tx(
        &mut self,
    ) -> DalResult<Vec<PendingCollateral>> {
        Ok(self
            .get_pending_field_collateral_for_eth_tx()
            .await?
            .into_iter()
            .map(PendingCollateral::Field)
            .chain(
                self.get_pending_tcb_info_collateral_for_eth_tx()
                    .await?
                    .into_iter()
                    .map(PendingCollateral::TcbInfo),
            )
            .collect())
    }

    pub async fn get_pending_field_collateral_for_eth_tx(
        &mut self,
    ) -> DalResult<Vec<PendingFieldCollateral>> {
        sqlx::query_as!(
            PendingFieldCollateral,
            r#"
            SELECT
                kind as "kind: _",
                calldata
            FROM
                tee_dcap_collateral
            WHERE
                eth_tx_id IS NULL 
                AND calldata IS NOT NULL
            ORDER BY updated ASC
        "#
        )
        .instrument("tee_dcap_collateral_get_pending_field_collateral_for_eth_tx")
        .report_latency()
        .fetch_all(self.storage)
        .await
    }

    pub async fn get_pending_tcb_info_collateral_for_eth_tx(
        &mut self,
    ) -> DalResult<Vec<PendingTcbInfoCollateral>> {
        sqlx::query_as!(
            PendingTcbInfoCollateral,
            r#"
            SELECT
                kind as "kind: _",
                fmspc,
                calldata
            FROM
                tee_dcap_collateral_tcb_info_json
            WHERE
                eth_tx_id IS NULL 
                AND calldata IS NOT NULL
            ORDER BY updated ASC
        "#
        )
        .instrument("tee_dcap_collateral_get_pending_tcb_info_collateral_for_eth_tx")
        .report_latency()
        .fetch_all(self.storage)
        .await
    }

    pub async fn get_expiring_collateral(
        &mut self,
        expires_within: PgInterval,
    ) -> DalResult<Vec<ExpiringCollateral>> {
        let mut tx = self.storage.start_transaction().await?;
        let out = tx
            .tee_dcap_collateral_dal()
            .get_expiring_collateral_internal(expires_within)
            .await?;
        tx.commit().await?;
        Ok(out)
    }

    async fn get_expiring_collateral_internal(
        &mut self,
        expires_within: PgInterval,
    ) -> DalResult<Vec<ExpiringCollateral>> {
        Ok(self
            .get_expiring_field_collateral(expires_within.clone())
            .await?
            .into_iter()
            .map(ExpiringCollateral::Field)
            .chain(
                self.get_expiring_tcb_info_collateral(expires_within)
                    .await?
                    .into_iter()
                    .map(ExpiringCollateral::TcbInfo),
            )
            .collect())
    }

    pub async fn get_expiring_field_collateral(
        &mut self,
        expires_within: PgInterval,
    ) -> DalResult<Vec<ExpiringFieldCollateral>> {
        sqlx::query_as!(
            ExpiringFieldCollateral,
            r#"
        SELECT
            kind as "kind: _",
            not_after
        FROM
            tee_dcap_collateral
        WHERE
            not_after <= transaction_timestamp() + $1
        ORDER BY not_after ASC
    "#,
            expires_within
        )
        .instrument("tee_dcap_collateral_get_expiring_field_collateral")
        .report_latency()
        .fetch_all(self.storage)
        .await
    }

    pub async fn get_expiring_tcb_info_collateral(
        &mut self,
        expires_within: PgInterval,
    ) -> DalResult<Vec<ExpiringTcbInfoCollateral>> {
        sqlx::query_as!(
            ExpiringTcbInfoCollateral,
            r#"
        SELECT
            kind as "kind: _",
            fmspc,
            not_after
        FROM
            tee_dcap_collateral_tcb_info_json
        WHERE
            not_after <= transaction_timestamp() + $1
        ORDER BY not_after ASC
    "#,
            expires_within
        )
        .instrument("tee_dcap_collateral_get_expiring_tcb_info_collateral")
        .report_latency()
        .fetch_all(self.storage)
        .await
    }

    pub async fn update_field(
        &mut self,
        kind: TeeDcapCollateralKind,
        sha256: &[u8],
        not_after: DateTime<Utc>,
        calldata: &[u8],
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE tee_dcap_collateral
            SET
                update_guard_set = NULL,
                sha256 = $2,
                not_after = $3,
                calldata = $4,
                eth_tx_id = NULL,
                updated = transaction_timestamp()
            WHERE
                kind = $1
            "#,
            kind as _,
            sha256,
            not_after,
            calldata
        )
        .instrument("tee_dcap_collateral_update_field")
        .with_arg("field", &kind)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn update_tcb_info(
        &mut self,
        kind: TeeDcapCollateralTcbInfoJsonKind,
        fmspc: &[u8],
        sha256: &[u8],
        not_after: DateTime<Utc>,
        calldata: &[u8],
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE tee_dcap_collateral_tcb_info_json
            SET
                update_guard_set = NULL,
                sha256 = $2,
                not_after = $3,
                calldata = $4,
                eth_tx_id = NULL,
                updated = transaction_timestamp()
            WHERE
                kind = $1 AND fmspc = $5
            "#,
            kind as _,
            sha256,
            not_after,
            calldata,
            fmspc
        )
        .instrument("tee_dcap_collateral_update_field")
        .with_arg("field", &kind)
        .with_arg("fmspc", &fmspc)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn set_field_eth_tx_id(
        &mut self,
        kind: TeeDcapCollateralKind,
        eth_tx_id: i32,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE tee_dcap_collateral
            SET
                eth_tx_id = $2,
                calldata = NULL
            WHERE
                kind = $1
            "#,
            kind as _,
            eth_tx_id
        )
        .instrument("tee_dcap_collateral_set_field_eth_tx_id")
        .with_arg("field", &kind)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn set_tcb_info_eth_tx_id(
        &mut self,
        kind: TeeDcapCollateralTcbInfoJsonKind,
        fmspc: &[u8],
        eth_tx_id: i32,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE tee_dcap_collateral_tcb_info_json
            SET
                eth_tx_id = $3,
                calldata = NULL
            WHERE
                kind = $1 AND fmspc = $2
            "#,
            kind as _,
            fmspc,
            eth_tx_id
        )
        .instrument("tee_dcap_collateral_set_tcb_info_eth_tx_id")
        .with_arg("field", &kind)
        .with_arg("fmspc", &fmspc)
        .execute(self.storage)
        .await?;
        Ok(())
    }
}

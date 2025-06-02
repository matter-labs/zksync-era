use std::fmt::{Display, Formatter};

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

type FMSPC = [u8; 6];

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum TeeDcapCollateralKind {
    RootCa,
    RootCrl,
    PckCa,
    PckCrl,
    SignCa,
    SgxTcbInfoJson(FMSPC),
    TdxTcbInfoJson(FMSPC),
    SgxQeIdentityJson,
    TdxQeIdentityJson,
}

impl Display for TeeDcapCollateralKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TeeDcapCollateralKind::RootCa => write!(f, "root_ca"),
            TeeDcapCollateralKind::RootCrl => write!(f, "root_crl"),
            TeeDcapCollateralKind::PckCa => write!(f, "pck_ca"),
            TeeDcapCollateralKind::PckCrl => write!(f, "pck_crl"),
            TeeDcapCollateralKind::SignCa => write!(f, "sign_ca"),
            TeeDcapCollateralKind::SgxTcbInfoJson(fmspc) => {
                write!(f, "sgx_tcb_{}", hex::encode(fmspc))
            }
            TeeDcapCollateralKind::TdxTcbInfoJson(fmspc) => {
                write!(f, "tdx_tcb_{}", hex::encode(fmspc))
            }
            TeeDcapCollateralKind::SgxQeIdentityJson => write!(f, "sgx_qe_identity"),
            TeeDcapCollateralKind::TdxQeIdentityJson => write!(f, "tdx_qe_identity"),
        }
    }
}

impl TryFrom<&str> for TeeDcapCollateralKind {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "root_ca" => Ok(TeeDcapCollateralKind::RootCa),
            "root_crl" => Ok(TeeDcapCollateralKind::RootCrl),
            "pck_ca" => Ok(TeeDcapCollateralKind::PckCa),
            "pck_crl" => Ok(TeeDcapCollateralKind::PckCrl),
            "sign_ca" => Ok(TeeDcapCollateralKind::SignCa),
            "sgx_qe_identity" => Ok(TeeDcapCollateralKind::SgxQeIdentityJson),
            "tdx_qe_identity" => Ok(TeeDcapCollateralKind::TdxQeIdentityJson),
            s if s.starts_with("sgx_tcb_") => {
                let hex_str = s.strip_prefix("sgx_tcb_").unwrap();
                let bytes =
                    hex::decode(hex_str).map_err(|e| format!("Invalid FMSPC hex: {}", e))?;
                if bytes.len() != 6 {
                    return Err(format!("FMSPC must be 6 bytes, got {}", bytes.len()));
                }
                let mut fmspc = [0u8; 6];
                fmspc.copy_from_slice(&bytes);
                Ok(TeeDcapCollateralKind::SgxTcbInfoJson(fmspc))
            }
            s if s.starts_with("tdx_tcb_") => {
                let hex_str = s.strip_prefix("tdx_tcb_").unwrap();
                let bytes =
                    hex::decode(hex_str).map_err(|e| format!("Invalid FMSPC hex: {}", e))?;
                if bytes.len() != 6 {
                    return Err(format!("FMSPC must be 6 bytes, got {}", bytes.len()));
                }
                let mut fmspc = [0u8; 6];
                fmspc.copy_from_slice(&bytes);
                Ok(TeeDcapCollateralKind::TdxTcbInfoJson(fmspc))
            }
            _ => Err(format!("Unknown collateral kind: {}", value)),
        }
    }
}

#[derive(Debug)]
pub struct TeeDcapCollateralDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}
impl TeeDcapCollateralDal<'_, '_> {
    /// Checks the status of a specific collateral against what's stored in the database
    pub async fn check_collateral_status(
        &mut self,
        kind: &TeeDcapCollateralKind,
        sha256: &[u8],
    ) -> DalResult<TeeDcapCollateralInfo> {
        let row = sqlx::query!(
            r#"
            SELECT
                sha256,
                not_after,
                eth_tx_id,
                update_guard_expires
            FROM tee_dcap_collateral
            WHERE kind = $1
            "#,
            kind.to_string()
        )
        .instrument("check_collateral_status")
        .report_latency()
        .fetch_optional(self.storage)
        .await?;

        match row {
            None => Ok(TeeDcapCollateralInfo::RecordMissing),
            Some(record) => {
                if record.sha256 == sha256 {
                    Ok(TeeDcapCollateralInfo::Matches)
                } else if record.eth_tx_id.is_some() {
                    // There's already an eth transaction in progress
                    let guard_expires = record
                        .update_guard_expires
                        .unwrap_or_else(|| Utc::now() + chrono::Duration::hours(1));
                    Ok(TeeDcapCollateralInfo::PendingUpdateBy(guard_expires))
                } else {
                    // Needs update
                    Ok(TeeDcapCollateralInfo::UpdateChainBy(record.not_after))
                }
            }
        }
    }

    /// Inserts or updates a collateral record
    pub async fn upsert_collateral(
        &mut self,
        kind: &TeeDcapCollateralKind,
        not_after: DateTime<Utc>,
        sha256: &[u8],
        calldata: &[u8],
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO tee_dcap_collateral (
                kind, not_after, sha256, updated, calldata, eth_tx_id
            )
            VALUES ($1, $2, $3, $4, $5, NULL)
            ON CONFLICT (kind) DO UPDATE SET
            not_after = excluded.not_after,
            sha256 = excluded.sha256,
            updated = excluded.updated,
            calldata = excluded.calldata,
            eth_tx_id = NULL
            "#,
            kind.to_string(),
            not_after,
            sha256,
            Utc::now(),
            calldata
        )
        .instrument("upsert_collateral")
        .with_arg("kind", &kind)
        .with_arg("not_after", &not_after)
        .with_arg("sha256", &sha256)
        .report_latency()
        .execute(self.storage)
        .await?;
        Ok(())
    }

    /// Gets all collateral records that need to be sent to Ethereum
    pub async fn get_pending_collateral_for_eth_tx(
        &mut self,
    ) -> DalResult<Vec<(TeeDcapCollateralKind, Vec<u8>)>> {
        let rows = sqlx::query!(
            r#"
            SELECT kind, calldata
            FROM tee_dcap_collateral
            WHERE eth_tx_id IS NULL
            "#
        )
        .instrument("get_pending_collateral_for_eth_tx")
        .report_latency()
        .fetch_all(self.storage)
        .await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                TeeDcapCollateralKind::try_from(row.kind.as_str())
                    .ok()
                    .map(|kind| (kind, row.calldata))
            })
            .collect())
    }

    /// Updates the eth_tx_id for a collateral record and sets the update guard
    pub async fn set_eth_tx_id(
        &mut self,
        kind: &TeeDcapCollateralKind,
        eth_tx_id: u32,
        guard_duration_hours: i64,
    ) -> DalResult<()> {
        let update_guard_expires = Utc::now() + chrono::Duration::hours(guard_duration_hours);

        sqlx::query!(
            r#"
            UPDATE tee_dcap_collateral
            SET eth_tx_id = $1, update_guard_expires = $2
            WHERE kind = $3
            "#,
            i32::try_from(eth_tx_id).unwrap(),
            update_guard_expires,
            kind.to_string()
        )
        .instrument("set_eth_tx_id")
        .report_latency()
        .execute(self.storage)
        .await?;
        Ok(())
    }

    /// Gets collateral records that may need retry (expired guard and no successful tx)
    pub async fn get_collateral_needing_retry(
        &mut self,
    ) -> DalResult<Vec<(TeeDcapCollateralKind, i32)>> {
        let rows = sqlx::query!(
            r#"
            SELECT c.kind, c.eth_tx_id
            FROM tee_dcap_collateral c
            WHERE
                c.eth_tx_id IS NOT NULL
                AND c.update_guard_expires < $1
                AND NOT EXISTS (
                    SELECT 1
                    FROM eth_txs_history h
                    WHERE
                        h.eth_tx_id = c.eth_tx_id
                        AND h.sent_successfully = TRUE
                )
            "#,
            Utc::now()
        )
        .instrument("get_collateral_needing_retry")
        .report_latency()
        .fetch_all(self.storage)
        .await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                // Since we're filtering by eth_tx_id IS NOT NULL, this should always be Some
                row.eth_tx_id.and_then(|id| {
                    TeeDcapCollateralKind::try_from(row.kind.as_str())
                        .ok()
                        .map(|kind| (kind, id))
                })
            })
            .collect())
    }

    /// Clears failed eth_tx_id to allow retry
    pub async fn clear_failed_eth_tx(&mut self, kind: &TeeDcapCollateralKind) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE tee_dcap_collateral
            SET eth_tx_id = NULL, update_guard_expires = NULL
            WHERE kind = $1
            "#,
            kind.to_string()
        )
        .instrument("clear_failed_eth_tx")
        .report_latency()
        .execute(self.storage)
        .await?;
        Ok(())
    }

    /// Gets all collateral records that are approaching expiration
    pub async fn get_expiring_collateral(
        &mut self,
        hours_before_expiry: i64,
    ) -> DalResult<Vec<(TeeDcapCollateralKind, DateTime<Utc>)>> {
        let threshold = Utc::now() + chrono::Duration::hours(hours_before_expiry);

        let rows = sqlx::query!(
            r#"
            SELECT kind, not_after
            FROM tee_dcap_collateral
            WHERE not_after <= $1
            ORDER BY not_after ASC
            "#,
            threshold
        )
        .instrument("get_expiring_collateral")
        .report_latency()
        .fetch_all(self.storage)
        .await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                TeeDcapCollateralKind::try_from(row.kind.as_str())
                    .ok()
                    .map(|kind| (kind, row.not_after))
            })
            .collect())
    }

    /// Gets a specific collateral record by kind
    pub async fn get_collateral_by_kind(
        &mut self,
        kind: &TeeDcapCollateralKind,
    ) -> DalResult<Option<(Vec<u8>, DateTime<Utc>, Vec<u8>)>> {
        let row = sqlx::query!(
            r#"
            SELECT sha256, not_after, calldata
            FROM tee_dcap_collateral
            WHERE kind = $1
            "#,
            kind.to_string()
        )
        .instrument("get_collateral_by_kind")
        .report_latency()
        .fetch_optional(self.storage)
        .await?;

        Ok(row.map(|r| (r.sha256, r.not_after, r.calldata)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collateral_kind_display_and_parse() {
        // Test simple variants
        let test_cases = vec![
            (TeeDcapCollateralKind::RootCa, "root_ca"),
            (TeeDcapCollateralKind::RootCrl, "root_crl"),
            (TeeDcapCollateralKind::PckCa, "pck_ca"),
            (TeeDcapCollateralKind::PckCrl, "pck_crl"),
            (TeeDcapCollateralKind::SignCa, "sign_ca"),
            (TeeDcapCollateralKind::SgxQeIdentityJson, "sgx_qe_identity"),
            (TeeDcapCollateralKind::TdxQeIdentityJson, "tdx_qe_identity"),
        ];

        for (kind, expected_str) in test_cases {
            assert_eq!(kind.to_string(), expected_str);
            let parsed = TeeDcapCollateralKind::try_from(expected_str).unwrap();
            assert_eq!(kind.to_string(), parsed.to_string());
        }

        // Test FMSPC variants
        let fmspc = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc];
        let sgx_tcb = TeeDcapCollateralKind::SgxTcbInfoJson(fmspc);
        assert_eq!(sgx_tcb.to_string(), "sgx_tcb_123456789abc");
        let parsed = TeeDcapCollateralKind::try_from("sgx_tcb_123456789abc").unwrap();
        match parsed {
            TeeDcapCollateralKind::SgxTcbInfoJson(parsed_fmspc) => {
                assert_eq!(parsed_fmspc, fmspc);
            }
            _ => panic!("Unexpected variant"),
        }

        let tdx_tcb = TeeDcapCollateralKind::TdxTcbInfoJson(fmspc);
        assert_eq!(tdx_tcb.to_string(), "tdx_tcb_123456789abc");
        let parsed = TeeDcapCollateralKind::try_from("tdx_tcb_123456789abc").unwrap();
        match parsed {
            TeeDcapCollateralKind::TdxTcbInfoJson(parsed_fmspc) => {
                assert_eq!(parsed_fmspc, fmspc);
            }
            _ => panic!("Unexpected variant"),
        }
    }

    #[test]
    fn test_collateral_kind_parse_errors() {
        // Invalid kind
        assert!(TeeDcapCollateralKind::try_from("invalid_kind").is_err());

        // Invalid hex in FMSPC
        assert!(TeeDcapCollateralKind::try_from("sgx_tcb_invalid").is_err());

        // Wrong FMSPC length
        assert!(TeeDcapCollateralKind::try_from("sgx_tcb_1234").is_err());
        assert!(TeeDcapCollateralKind::try_from("sgx_tcb_12345678901234").is_err());
    }
}

use std::{ops::Sub, sync::Arc};

use chrono::{DateTime, TimeDelta, Utc};
use intel_dcap_api::{ApiVersion, EnclaveIdentityResponse, TcbInfoResponse};
use serde_json::Value;
use sha2::Digest;
use teepot::quote::TEEType;
use tokio::{select, sync::watch};
use zksync_config::configs::TeeProofDataHandlerConfig;
use zksync_dal::{
    tee_dcap_collateral_dal::{TeeDcapCollateralInfo, TeeDcapCollateralKind},
    Connection, ConnectionPool, Core, CoreDal,
};
use zksync_object_store::ObjectStore;
use zksync_types::{commitment::L1BatchCommitmentMode, L2ChainId};

use crate::errors::{TeeProcessorContext, TeeProcessorError};

pub(crate) async fn updater(
    blob_store: Arc<dyn ObjectStore>,
    mut connection_pool: ConnectionPool<Core>,
    config: TeeProofDataHandlerConfig,
    commitment_mode: L1BatchCommitmentMode,
    l2_chain_id: L2ChainId,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let mut interval = tokio::time::interval(config.dcap_collateral_refresh_in_hours);
    let mut connection = connection_pool
        .connection_tagged("tee_dcap_collateral_updater")
        .await?;
    loop {
        select! {
            _ = interval.tick() => {}
            signal = stop_receiver.changed() => {
                if signal.is_err() {
                    tracing::warn!("Stop signal sender for tee dcap collateral updater was dropped without sending a signal");
                }
                tracing::info!("Stop signal received, tee dcap collateral updater is shutting down");
                return Ok(());
            }
        }
        let mut dal = connection.tee_dcap_collateral_dal();
        // do work here
    }
}

pub(crate) fn get_next_update(
    tcbinfo_or_qe_identity_val: &Value,
) -> Result<DateTime<Utc>, TeeProcessorError> {
    let next_update = tcbinfo_or_qe_identity_val
        .get("nextUpdate")
        .context("Failed to get nextUpdate")?;
    let next_update = next_update.as_str().context("nextUpdate is not a string")?;
    let next_update =
        chrono::DateTime::parse_from_rfc3339(next_update).context("Failed to parse nextUpdate")?;
    // Let's try to refresh it at least 7 days before it expires.
    let next_update = next_update.sub(TimeDelta::days(7));
    Ok(next_update.to_utc())
}

pub(crate) async fn update_collateral_for_quote(
    connection: &mut Connection<'_, Core>,
    quote_bytes: &[u8],
) -> Result<(), TeeProcessorError> {
    let quote = teepot::quote::Quote::parse(quote_bytes).context("Failed to parse quote")?;
    let fmspc = quote.fmspc().context("Failed to get FMSPC")?;
    let fmspc = hex::encode(&fmspc);
    let (tcbinfo_resp, tcb_info_field, qe_identity_resp, qe_identity_field) = match quote.tee_type()
    {
        TEEType::SGX => {
            // For the automata contracts, we need version 3 of Intel DCAP API for SGX.
            let client = intel_dcap_api::ApiClient::new_with_version(ApiVersion::V3)
                .context("Failed to create Intel DCAP API client")?;
            let tcbinfo = client
                .get_sgx_tcb_info(&fmspc, None, None)
                .await
                .context("Failed to get SGX TCB info")?;
            let qe_identity = client
                .get_sgx_qe_identity(None, None)
                .await
                .context("Failed to get SGX QE identity")?;
            (
                tcbinfo,
                TeeDcapCollateralKind::SgxTcbInfoJson,
                qe_identity,
                TeeDcapCollateralKind::SgxQeIdentityJson,
            )
        }
        TEEType::TDX => {
            // For the automata contracts, we need version 4 of Intel DCAP API for TDX.
            let client = intel_dcap_api::ApiClient::new_with_version(ApiVersion::V4)
                .context("Failed to create Intel DCAP API client")?;
            let tcbinfo = client
                .get_tdx_tcb_info(&fmspc, None, None)
                .await
                .context("Failed to get TDX TCB info")?;
            let qe_identity = client
                .get_tdx_qe_identity(None, None)
                .await
                .context("Failed to get TDX QE identity")?;
            (
                tcbinfo,
                TeeDcapCollateralKind::TdxTcbInfoJson,
                qe_identity,
                TeeDcapCollateralKind::TdxQeIdentityJson,
            )
        }
        _ => {
            return Err(TeeProcessorError::GeneralError(
                "Not supported TEE type".into(),
            ));
        }
    };

    let TcbInfoResponse {
        tcb_info_json,
        issuer_chain: tcb_issuer_chain,
    } = tcbinfo_resp;

    let EnclaveIdentityResponse {
        enclave_identity_json,
        issuer_chain: pck_issuer_chain,
    } = qe_identity_resp;

    let tcb_info_hash = sha2::Sha256::new()
        .chain_update(tcb_info_json.as_bytes())
        .finalize();

    let qe_identity_hash = sha2::Sha256::new()
        .chain_update(enclave_identity_json.as_bytes())
        .finalize_reset();

    let mut dal = connection.tee_dcap_collateral_dal();

    if !matches!(
        dal.field_is_current(tcb_info_field, tcb_info_hash.as_slice())
            .await?,
        TeeDcapCollateralInfo::Matches
    ) {
        let tcb_info_val = serde_json::from_str::<serde_json::Value>(tcb_info_json.as_str())
            .context("Failed to parse TCB info")?;
        let tcb_info_val = tcb_info_val
            .get("tcbInfo")
            .context("Failed to get enclave identity")?;
        let not_after = get_next_update(&tcb_info_val)?;

        dal.field_updated(tcb_info_field, &tcb_info_hash, not_after)
            .await?;
    }

    if !matches!(
        dal.field_is_current(qe_identity_field, qe_identity_hash.as_slice())
            .await?,
        TeeDcapCollateralInfo::Matches
    ) {
        let enclave_identity_val =
            serde_json::from_str::<serde_json::Value>(enclave_identity_json.as_str())
                .context("Failed to parse enclave identity")?;
        let enclave_identity_val = enclave_identity_val
            .get("enclaveIdentity")
            .context("Failed to get enclave identity")?;
        let not_after = get_next_update(&enclave_identity_val)?;

        dal.field_updated(qe_identity_field, &qe_identity_hash, not_after)
            .await?;
    }

    Ok(())
}

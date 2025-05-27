use std::{ops::Sub, sync::Arc};

use chrono::{DateTime, Duration, TimeDelta, Utc};
use intel_dcap_api::{ApiClient, ApiVersion, CaType, CrlEncoding, PckCrlResponse, TcbInfoResponse};
use serde_json::Value;
use sha2::Digest;
use teepot::quote::TEEType;
use tokio::{select, sync::watch};
use x509_cert::{
    crl::CertificateList,
    der::{Decode, Encode},
};
use zksync_config::configs::TeeProofDataHandlerConfig;
use zksync_dal::{
    tee_dcap_collateral_dal::{TeeDcapCollateralDal, TeeDcapCollateralInfo, TeeDcapCollateralKind},
    Connection, ConnectionPool, Core, CoreDal,
};
use zksync_object_store::ObjectStore;
use zksync_types::{commitment::L1BatchCommitmentMode, L2ChainId};

use crate::{
    errors::{TeeProcessorContext, TeeProcessorError},
    tee_contract::{EnclaveId, TeeFunctions, CA},
};

#[derive(Debug, Default, Clone)]
struct CollateralState {
    functions: TeeFunctions,
    root_ca_hash: Vec<u8>,
    pck_ca_hash: Vec<u8>,
    pck_crl_hash: Vec<u8>,
    sgx_qe_identity_hash: Vec<u8>,
    tdx_qe_identity_hash: Vec<u8>,
}

pub(crate) async fn updater(
    _blob_store: Arc<dyn ObjectStore>,
    connection_pool: ConnectionPool<Core>,
    config: TeeProofDataHandlerConfig,
    _commitment_mode: L1BatchCommitmentMode,
    _l2_chain_id: L2ChainId,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let mut interval = tokio::time::interval(config.dcap_collateral_refresh_in_hours);
    let mut connection = connection_pool
        .connection_tagged("tee_dcap_collateral_updater")
        .await?;

    let mut state = CollateralState::default();

    loop {
        select! {
            _ = interval.tick() => {}
            signal = stop_receiver.changed() => {
                if signal.is_err() {
                    tracing::warn!("Stop signal sender for tee dcap collateral updater was dropped without sending a signal");
                }
                tracing::info!("Stop signal received; tee dcap collateral updater is shutting down");
                return Ok(());
            }
        }
        let mut dal = connection.tee_dcap_collateral_dal();
        update_collateral(&mut state, &mut dal, &config).await?
    }
}

async fn update_collateral(
    state: &mut CollateralState,
    dal: &mut TeeDcapCollateralDal<'_, '_>,
    _config: &TeeProofDataHandlerConfig,
) -> Result<(), TeeProcessorError> {
    // TODO: TEE - make config
    let hours_before_expiry = 24 * 7;

    for (kind, when) in dal
        .get_expiring_collateral(hours_before_expiry)
        .await?
        .iter()
    {
        match kind {
            TeeDcapCollateralKind::PckCrl
            | TeeDcapCollateralKind::RootCa
            | TeeDcapCollateralKind::PckCa => {
                let client = ApiClient::new().context("Failed to create Intel DCAP API client")?;

                let PckCrlResponse {
                    crl_data,
                    issuer_chain,
                } = client
                    .get_pck_crl(CaType::Platform, Some(CrlEncoding::Der))
                    .await
                    .context("Failed to get PCK CRL")?;

                let mut certs = x509_cert::certificate::CertificateInner::<
                    x509_cert::certificate::Rfc5280,
                >::load_pem_chain(issuer_chain.as_bytes())
                .map_err(|_| {
                    TeeProcessorError::GeneralError("Could not load a PEM chain".into())
                })?;

                if !certs.len() == 2 {
                    let msg = format!("Expected 2 certificates in the chain, got {}", certs.len());
                    tracing::error!(msg);
                    return Err(TeeProcessorError::GeneralError(msg));
                }

                let pck_cert = certs.pop().unwrap();
                let root_cert = certs.pop().unwrap();

                let hash = root_cert.signature.raw_bytes().to_vec();

                if !matches!(
                    dal.check_collateral_status(&TeeDcapCollateralKind::RootCa, &hash)
                        .await?,
                    TeeDcapCollateralInfo::Matches
                ) {
                    let not_after = root_cert.tbs_certificate.validity.not_after;
                    let calldata = state
                        .functions
                        .upsert_pcs_certificates(vec![CA::ROOT], vec![root_cert.to_der().unwrap()])
                        .unwrap();
                    dal.upsert_collateral(
                        &TeeDcapCollateralKind::RootCa,
                        not_after.to_system_time().into(),
                        hash.as_slice(),
                        &calldata,
                    )
                    .await?;
                    state.root_ca_hash = hash;
                }

                let hash = pck_cert.signature.raw_bytes().to_vec();

                if !matches!(
                    dal.check_collateral_status(&TeeDcapCollateralKind::PckCa, &hash)
                        .await?,
                    TeeDcapCollateralInfo::Matches
                ) {
                    let not_after = pck_cert.tbs_certificate.validity.not_after;
                    let calldata = state
                        .functions
                        .upsert_pcs_certificates(
                            vec![CA::PLATFORM],
                            vec![pck_cert.to_der().unwrap()],
                        )
                        .unwrap();

                    dal.upsert_collateral(
                        &TeeDcapCollateralKind::RootCa,
                        not_after.to_system_time().into(),
                        hash.as_slice(),
                        &calldata,
                    )
                    .await?;
                    state.pck_ca_hash = hash;
                }

                let hash = sha2::Sha256::new()
                    .chain_update(&crl_data)
                    .finalize_reset()
                    .to_vec();

                if !matches!(
                    dal.check_collateral_status(&TeeDcapCollateralKind::PckCrl, &hash)
                        .await?,
                    TeeDcapCollateralInfo::Matches
                ) {
                    let crl =
                        CertificateList::from_der(&crl_data).context("Failed to parse CRL")?;
                    let not_after = crl
                        .tbs_cert_list
                        .next_update
                        .map(|t| t.to_system_time().into())
                        .unwrap_or_else(|| Utc::now() + Duration::days(365));

                    let calldata = state
                        .functions
                        .upsert_pck_crl(CA::PLATFORM, pck_cert.to_der().unwrap())
                        .unwrap();

                    dal.upsert_collateral(
                        &TeeDcapCollateralKind::PckCrl,
                        not_after,
                        &hash,
                        &calldata,
                    )
                    .await?;

                    state.pck_crl_hash = hash;
                }
            }
            TeeDcapCollateralKind::SgxQeIdentityJson => {
                let client = ApiClient::new_with_version(ApiVersion::V3)
                    .context("Failed to create Intel DCAP API client")?;

                let qe_identity = client
                    .get_sgx_qe_identity(None, None)
                    .await
                    .context("Failed to get SGX QE identity")?;

                let qe_identity_hash = sha2::Sha256::new()
                    .chain_update(qe_identity.enclave_identity_json.as_bytes())
                    .finalize_reset()
                    .to_vec();

                let enclave_identity_val = serde_json::from_str::<serde_json::Value>(
                    qe_identity.enclave_identity_json.as_str(),
                )
                .context("Failed to parse enclave identity")?;

                let signature = enclave_identity_val
                    .get("signature")
                    .unwrap()
                    .as_str()
                    .unwrap();
                let signature = hex::decode(signature).unwrap();

                let enclave_identity_val = enclave_identity_val
                    .get("enclaveIdentity")
                    .context("Failed to get enclave identity")?;

                let not_after = get_next_update(&enclave_identity_val)?;
                let id =
                    EnclaveId::try_from(enclave_identity_val.get("id").unwrap().as_str().unwrap())
                        .unwrap();
                let version = enclave_identity_val
                    .get("version")
                    .unwrap()
                    .as_u64()
                    .unwrap();
                let calldata = state
                    .functions
                    .upsert_enclave_identity(
                        id,
                        version,
                        qe_identity.enclave_identity_json,
                        signature,
                    )
                    .unwrap();

                dal.upsert_collateral(
                    &TeeDcapCollateralKind::SgxQeIdentityJson,
                    not_after,
                    &qe_identity_hash,
                    &calldata,
                )
                .await?;

                state.sgx_qe_identity_hash = qe_identity_hash;
            }
            TeeDcapCollateralKind::TdxQeIdentityJson => {
                let client = ApiClient::new_with_version(ApiVersion::V4)
                    .context("Failed to create Intel DCAP API client")?;

                let qe_identity = client
                    .get_sgx_qe_identity(None, None)
                    .await
                    .context("Failed to get TDX QE identity")?;

                let qe_identity_hash = sha2::Sha256::new()
                    .chain_update(qe_identity.enclave_identity_json.as_bytes())
                    .finalize_reset()
                    .to_vec();

                let enclave_identity_val = serde_json::from_str::<serde_json::Value>(
                    qe_identity.enclave_identity_json.as_str(),
                )
                .context("Failed to parse enclave identity")?;

                let signature = enclave_identity_val
                    .get("signature")
                    .unwrap()
                    .as_str()
                    .unwrap();
                let signature = hex::decode(signature).unwrap();

                let enclave_identity_val = enclave_identity_val
                    .get("enclaveIdentity")
                    .context("Failed to get enclave identity")?;

                let not_after = get_next_update(&enclave_identity_val)?;
                let id =
                    EnclaveId::try_from(enclave_identity_val.get("id").unwrap().as_str().unwrap())
                        .unwrap();
                let version = enclave_identity_val
                    .get("version")
                    .unwrap()
                    .as_u64()
                    .unwrap();
                let calldata = state
                    .functions
                    .upsert_enclave_identity(
                        id,
                        version,
                        enclave_identity_val.as_str().unwrap().into(),
                        signature,
                    )
                    .unwrap();

                dal.upsert_collateral(
                    &TeeDcapCollateralKind::TdxQeIdentityJson,
                    not_after,
                    &qe_identity_hash,
                    &calldata,
                )
                .await?;

                state.tdx_qe_identity_hash = qe_identity_hash;
            }
            TeeDcapCollateralKind::SgxTcbInfoJson(fmspc) => {}
        }
    }

    Ok(())
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
    let fmspc_hex = hex::encode(&fmspc);
    let (tcbinfo_resp, tcb_info_field) = match quote.tee_type() {
        TEEType::SGX => {
            // For the automata contracts, we need version 3 of Intel DCAP API for SGX.
            let client = ApiClient::new_with_version(ApiVersion::V3)
                .context("Failed to create Intel DCAP API client")?;
            let tcbinfo = client
                .get_sgx_tcb_info(&fmspc_hex, None, None)
                .await
                .context("Failed to get SGX TCB info")?;
            (tcbinfo, TeeDcapCollateralKind::SgxTcbInfoJson(fmspc))
        }
        TEEType::TDX => {
            // For the automata contracts, we need version 4 of Intel DCAP API for TDX.
            let client = ApiClient::new_with_version(ApiVersion::V4)
                .context("Failed to create Intel DCAP API client")?;
            let tcbinfo = client
                .get_tdx_tcb_info(&fmspc_hex, None, None)
                .await
                .context("Failed to get TDX TCB info")?;
            (tcbinfo, TeeDcapCollateralKind::TdxTcbInfoJson(fmspc))
        }
        _ => {
            return Err(TeeProcessorError::GeneralError(
                "Not supported TEE type".into(),
            ));
        }
    };

    let TcbInfoResponse { tcb_info_json, .. } = tcbinfo_resp;

    let tcb_info_hash = sha2::Sha256::new()
        .chain_update(tcb_info_json.as_bytes())
        .finalize();

    let mut dal = connection.tee_dcap_collateral_dal();

    if !matches!(
        dal.check_collateral_status(&tcb_info_field, tcb_info_hash.as_slice())
            .await?,
        TeeDcapCollateralInfo::Matches
    ) {
        let tcb_info_val = serde_json::from_str::<serde_json::Value>(tcb_info_json.as_str())
            .context("Failed to parse TCB info")?;
        let signature = tcb_info_val
            .get("signature")
            .context("Failed to get signature from TCB info")?;
        let signature = hex::decode(signature.as_str().unwrap()).unwrap();

        let tcb_info_val = tcb_info_val
            .get("tcbInfo")
            .context("Failed to get tcbInfo")?;
        let not_after = get_next_update(&tcb_info_val)?;

        let calldata = TeeFunctions::default()
            .upsert_fmspc_tcb(tcb_info_val.as_str().unwrap().into(), signature)
            .unwrap();
        dal.upsert_collateral(
            &tcb_info_field,
            not_after,
            tcb_info_hash.as_slice(),
            &calldata,
        )
        .await?;
    }

    Ok(())
}

use chrono::{DateTime, Duration, Utc};
use intel_dcap_api::{
    ApiClient, ApiVersion, CaType, CrlEncoding, EnclaveIdentityResponse, PckCrlResponse,
    TcbInfoResponse,
};
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
    tee_dcap_collateral_dal::{
        ExpiringCollateral, ExpiringFieldCollateral, ExpiringTcbInfoCollateral,
        TeeDcapCollateralDal, TeeDcapCollateralInfo, TeeDcapCollateralKind,
        TeeDcapCollateralTcbInfoJsonKind,
    },
    ConnectionPool, Core, CoreDal,
};

use crate::{
    errors::{TeeProcessorContext, TeeProcessorError},
    tee_contract::{EnclaveId, TeeFunctions, CA},
};

const INTEL_ROOT_CA_CRL_URL: &str =
    "https://certificates.trustedservices.intel.com/IntelSGXRootCA.der";

pub struct CollateralUpdater<'a, 'b, 'c> {
    functions: &'a TeeFunctions,
    dal: TeeDcapCollateralDal<'b, 'c>,
    api_client_v3: ApiClient,
    api_client_v4: ApiClient,
}

pub(crate) async fn updater(
    connection_pool: ConnectionPool<Core>,
    config: TeeProofDataHandlerConfig,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let mut interval = tokio::time::interval(config.dcap_collateral_refresh_in_hours);
    let mut connection = connection_pool
        .connection_tagged("tee_dcap_collateral_updater")
        .await?;

    // Init once, if DB empty
    let dal = connection.tee_dcap_collateral_dal();
    let functions = TeeFunctions::default();
    let mut updater =
        CollateralUpdater::new(&functions, dal).context("Failed to create collateral updater")?;
    updater.update_certs().await?;
    updater.update_sgx_qe_identity().await?;
    updater.update_tdx_qe_identity().await?;

    loop {
        updater.update_collateral().await?;

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
    }
}

impl<'a, 'b, 'c> CollateralUpdater<'a, 'b, 'c> {
    pub fn new(
        functions: &'a TeeFunctions,
        dal: TeeDcapCollateralDal<'b, 'c>,
    ) -> Result<Self, TeeProcessorError> {
        let api_client_v4 = ApiClient::new_with_version(ApiVersion::V4)
            .context("Failed to create Intel DCAP API client V4")?;
        let api_client_v3 = ApiClient::new_with_version(ApiVersion::V3)
            .context("Failed to create Intel DCAP API client V3")?;

        Ok(Self {
            functions,
            dal,
            api_client_v3,
            api_client_v4,
        })
    }

    async fn update_collateral(&mut self) -> Result<(), TeeProcessorError> {
        for expiring_collateral in self
            .dal
            .get_expiring_collateral(TeeDcapCollateralDal::DEFAULT_EXPIRES_WITHIN)
            .await?
            .iter()
        {
            match expiring_collateral {
                ExpiringCollateral::Field(ExpiringFieldCollateral { kind, .. }) => match kind {
                    TeeDcapCollateralKind::RootCa
                    | TeeDcapCollateralKind::PckCa
                    | TeeDcapCollateralKind::PckCrl => self.update_certs().await?,
                    TeeDcapCollateralKind::RootCrl => {
                        self.update_root_crl().await?;
                    }
                    TeeDcapCollateralKind::SignCa => {
                        // should have happened automatically via SgxQeIdentityJson or TdxQeIdentityJson
                        return Err(TeeProcessorError::GeneralError(
                            "TEE Signing CA outdated!".into(),
                        ));
                    }
                    TeeDcapCollateralKind::SgxQeIdentityJson => {
                        self.update_sgx_qe_identity().await?;
                    }
                    TeeDcapCollateralKind::TdxQeIdentityJson => {
                        self.update_tdx_qe_identity().await?;
                    }
                },
                ExpiringCollateral::TcbInfo(ExpiringTcbInfoCollateral { kind, fmspc, .. }) => {
                    match kind {
                        TeeDcapCollateralTcbInfoJsonKind::SgxTcbInfoJson => {
                            self.update_tcb_info(fmspc, TEEType::SGX).await?;
                        }
                        TeeDcapCollateralTcbInfoJsonKind::TdxTcbInfoJson => {
                            self.update_tcb_info(fmspc, TEEType::TDX).await?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn update_root_crl(&mut self) -> Result<(), TeeProcessorError> {
        let crl_data = reqwest::get(INTEL_ROOT_CA_CRL_URL)
            .await
            .context("Failed to get root ca crl URL")?
            .bytes()
            .await
            .context("Failed to convert request to bytes")?
            .to_vec();

        let hash = sha2::Sha256::new()
            .chain_update(&crl_data)
            .finalize_reset()
            .to_vec();

        if !matches!(
            self.dal
                .field_is_current(
                    TeeDcapCollateralKind::RootCrl,
                    &hash,
                    TeeDcapCollateralDal::DEFAULT_TIMEOUT
                )
                .await?,
            TeeDcapCollateralInfo::Matches
        ) {
            let crl = CertificateList::from_der(&crl_data).context("Failed to parse CRL")?;
            let not_after = crl
                .tbs_cert_list
                .next_update
                .map(|t| t.to_system_time().into())
                .unwrap_or_else(|| Utc::now() + Duration::days(30));

            tracing::info!("Updating collateral: root_crl = {}", hex::encode(&crl_data));

            let calldata = self.functions.upsert_root_ca_crl(crl_data).unwrap();

            self.dal
                .update_field(TeeDcapCollateralKind::RootCrl, &hash, not_after, &calldata)
                .await?;
        }

        Ok(())
    }

    async fn update_certs(&mut self) -> Result<(), TeeProcessorError> {
        let PckCrlResponse {
            crl_data,
            issuer_chain,
        } = self
            .api_client_v4
            .get_pck_crl(CaType::Platform, Some(CrlEncoding::Der))
            .await
            .context("Failed to get PCK CRL")?;

        let certs = x509_cert::certificate::CertificateInner::<
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

        let root_cert = certs
            .iter()
            .find(|cert| cert.tbs_certificate.subject.to_string().contains("Root CA"))
            .unwrap();

        let pck_cert = certs
            .iter()
            .find(|cert| {
                cert.tbs_certificate
                    .subject
                    .to_string()
                    .contains("Platform CA")
            })
            .unwrap();

        let hash = root_cert.signature.raw_bytes().to_vec();

        if !matches!(
            self.dal
                .field_is_current(
                    TeeDcapCollateralKind::RootCa,
                    &hash,
                    TeeDcapCollateralDal::DEFAULT_TIMEOUT
                )
                .await?,
            TeeDcapCollateralInfo::Matches
        ) {
            let not_after = root_cert
                .tbs_certificate
                .validity
                .not_after
                .to_system_time();
            let cert_der = root_cert.to_der().expect("Failed to serialize root cert");
            tracing::info!("Updating collateral: {:?}", TeeDcapCollateralKind::RootCa);
            tracing::info!("Updating collateral: cert_der = {}", hex::encode(&cert_der));
            let calldata = self
                .functions
                .upsert_root_certificate(cert_der)
                .expect("Failed to create calldata for root cert");
            self.dal
                .update_field(
                    TeeDcapCollateralKind::RootCa,
                    &hash,
                    not_after.into(),
                    &calldata,
                )
                .await?;
        }

        self.update_root_crl().await?;

        let hash = pck_cert.signature.raw_bytes().to_vec();

        if !matches!(
            self.dal
                .field_is_current(
                    TeeDcapCollateralKind::PckCa,
                    &hash,
                    TeeDcapCollateralDal::DEFAULT_TIMEOUT
                )
                .await?,
            TeeDcapCollateralInfo::Matches
        ) {
            let not_after = pck_cert.tbs_certificate.validity.not_after.to_system_time();
            let cert_der = pck_cert.to_der().unwrap();

            tracing::info!("Updating collateral: {:?}", TeeDcapCollateralKind::PckCa);
            tracing::info!("Updating collateral: cert_der = {}", hex::encode(&cert_der));

            let calldata = self
                .functions
                .upsert_platform_certificate(cert_der)
                .unwrap();

            self.dal
                .update_field(
                    TeeDcapCollateralKind::PckCa,
                    &hash,
                    not_after.into(),
                    &calldata,
                )
                .await?;
        }

        let hash = sha2::Sha256::new()
            .chain_update(&crl_data)
            .finalize_reset()
            .to_vec();

        if !matches!(
            self.dal
                .field_is_current(
                    TeeDcapCollateralKind::PckCrl,
                    &hash,
                    TeeDcapCollateralDal::DEFAULT_TIMEOUT
                )
                .await?,
            TeeDcapCollateralInfo::Matches
        ) {
            let crl = CertificateList::from_der(&crl_data).context("Failed to parse CRL")?;
            let not_after = crl
                .tbs_cert_list
                .next_update
                .map(|t| t.to_system_time().into())
                .unwrap_or_else(|| Utc::now() + Duration::days(30));

            tracing::info!("Updating collateral: {:?}", TeeDcapCollateralKind::PckCrl);
            tracing::info!("Updating collateral: cert_der = {}", hex::encode(&crl_data));

            let calldata = self
                .functions
                .upsert_pck_crl(CA::PLATFORM, crl_data)
                .unwrap();

            self.dal
                .update_field(TeeDcapCollateralKind::PckCrl, &hash, not_after, &calldata)
                .await?;
        }

        Ok(())
    }

    async fn update_tdx_qe_identity(&mut self) -> Result<(), TeeProcessorError> {
        let qe_identity = self
            .api_client_v4
            .get_tdx_qe_identity(None, None)
            .await
            .context("Failed to get TDX QE identity")?;

        let qe_identity_hash = sha2::Sha256::new()
            .chain_update(qe_identity.enclave_identity_json.as_bytes())
            .finalize_reset()
            .to_vec();

        if !matches!(
            self.dal
                .field_is_current(
                    TeeDcapCollateralKind::TdxQeIdentityJson,
                    &qe_identity_hash,
                    TeeDcapCollateralDal::DEFAULT_TIMEOUT
                )
                .await?,
            TeeDcapCollateralInfo::Matches
        ) {
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

            let not_after = get_next_update(enclave_identity_val)?;
            let id = EnclaveId::try_from(enclave_identity_val.get("id").unwrap().as_str().unwrap())
                .unwrap();

            tracing::info!("Updating collateral: {}", qe_identity.enclave_identity_json);
            let body = extract_json_body(&qe_identity.enclave_identity_json, "enclaveIdentity")?;
            tracing::info!("body: {}", body);

            let calldata = self
                .functions
                .upsert_enclave_identity(id, 4, body, signature)
                .expect("Failed to create calldata for enclave identity");

            self.dal
                .update_field(
                    TeeDcapCollateralKind::TdxQeIdentityJson,
                    &qe_identity_hash,
                    not_after,
                    &calldata,
                )
                .await?;
        }
        Ok(())
    }

    async fn update_sgx_qe_identity(&mut self) -> Result<(), TeeProcessorError> {
        let qe_identity_resp = self
            .api_client_v3
            .get_sgx_qe_identity(None, None)
            .await
            .context("Failed to get SGX QE identity")?;

        let EnclaveIdentityResponse {
            enclave_identity_json,
            issuer_chain,
        } = qe_identity_resp;

        let qe_identity_hash = sha2::Sha256::new()
            .chain_update(enclave_identity_json.as_bytes())
            .finalize_reset()
            .to_vec();

        if !matches!(
            self.dal
                .field_is_current(
                    TeeDcapCollateralKind::SgxQeIdentityJson,
                    &qe_identity_hash,
                    TeeDcapCollateralDal::DEFAULT_TIMEOUT
                )
                .await?,
            TeeDcapCollateralInfo::Matches
        ) {
            self.update_signing_ca(issuer_chain).await?;

            let enclave_identity_val =
                serde_json::from_str::<serde_json::Value>(enclave_identity_json.as_str())
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

            let not_after = get_next_update(enclave_identity_val)?;
            let id = EnclaveId::try_from(enclave_identity_val.get("id").unwrap().as_str().unwrap())
                .unwrap();

            tracing::info!("Updating collateral: {}", enclave_identity_json);
            let body = extract_json_body(&enclave_identity_json, "enclaveIdentity")?;
            tracing::info!("body: {}", body);

            let calldata = self
                .functions
                .upsert_enclave_identity(id, 3, body, signature)
                .unwrap();

            self.dal
                .update_field(
                    TeeDcapCollateralKind::SgxQeIdentityJson,
                    &qe_identity_hash,
                    not_after,
                    &calldata,
                )
                .await?;
        }
        Ok(())
    }

    pub(crate) async fn update_tcb_info(
        &mut self,
        fmspc: &[u8],
        tee_type: TEEType,
    ) -> Result<(), TeeProcessorError> {
        let fmspc_hex = hex::encode(fmspc);
        let (tcbinfo_resp, tcb_info_field) = match tee_type {
            TEEType::SGX => {
                // For the automata contracts, we need version 3 of Intel DCAP API for SGX.
                let client = &self.api_client_v3;
                let tcbinfo = client
                    .get_sgx_tcb_info(&fmspc_hex, None, None)
                    .await
                    .context("Failed to get SGX TCB info")?;
                (tcbinfo, TeeDcapCollateralTcbInfoJsonKind::SgxTcbInfoJson)
            }
            TEEType::TDX => {
                // For the automata contracts, we need version 4 of Intel DCAP API for TDX.
                let client = &self.api_client_v4;
                let tcbinfo = client
                    .get_tdx_tcb_info(&fmspc_hex, None, None)
                    .await
                    .context("Failed to get TDX TCB info")?;
                (tcbinfo, TeeDcapCollateralTcbInfoJsonKind::TdxTcbInfoJson)
            }
            _ => {
                return Err(TeeProcessorError::GeneralError(
                    "Not supported TEE type".into(),
                ));
            }
        };

        let TcbInfoResponse {
            tcb_info_json,
            issuer_chain,
        } = tcbinfo_resp;

        let tcb_info_hash = sha2::Sha256::new()
            .chain_update(tcb_info_json.as_bytes())
            .finalize();

        if !matches!(
            self.dal
                .tcb_info_is_current(
                    tcb_info_field,
                    fmspc,
                    tcb_info_hash.as_slice(),
                    TeeDcapCollateralDal::DEFAULT_TIMEOUT
                )
                .await?,
            TeeDcapCollateralInfo::Matches
        ) {
            self.update_signing_ca(issuer_chain).await?;

            let tcb_info_val = serde_json::from_str::<serde_json::Value>(tcb_info_json.as_str())
                .context("Failed to parse TCB info")?;
            let signature = tcb_info_val
                .get("signature")
                .context("Failed to get signature from TCB info")?;
            let signature = hex::decode(signature.as_str().unwrap()).unwrap();

            let tcb_info_val = tcb_info_val
                .get("tcbInfo")
                .context("Failed to get tcbInfo")?;
            let not_after = get_next_update(tcb_info_val)?;

            tracing::info!("Updating collateral: {}", tcb_info_json);
            let body = extract_json_body(&tcb_info_json, "tcbInfo")?;
            tracing::info!("body: {}", body);

            let calldata = self.functions.upsert_fmspc_tcb(body, signature).unwrap();
            self.dal
                .update_tcb_info(
                    tcb_info_field,
                    fmspc,
                    tcb_info_hash.as_slice(),
                    not_after,
                    &calldata,
                )
                .await?;
        }

        Ok(())
    }

    async fn update_signing_ca(&mut self, issuer_chain: String) -> Result<(), TeeProcessorError> {
        let certs = x509_cert::certificate::CertificateInner::<
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

        let sign_cert = certs
            .iter()
            .find(|cert| cert.tbs_certificate.subject.to_string().contains("Signing"))
            .unwrap();

        let hash = sign_cert.signature.raw_bytes().to_vec();

        if !matches!(
            self.dal
                .field_is_current(
                    TeeDcapCollateralKind::SignCa,
                    &hash,
                    TeeDcapCollateralDal::DEFAULT_TIMEOUT
                )
                .await?,
            TeeDcapCollateralInfo::Matches
        ) {
            let not_after = sign_cert.tbs_certificate.validity.not_after;

            let cert_der = sign_cert.to_der().unwrap();

            tracing::info!("Updating collateral: {:?}", TeeDcapCollateralKind::SignCa);
            tracing::info!("Updating collateral: cert_der = {}", hex::encode(&cert_der));

            let calldata = self.functions.upsert_signing_certificate(cert_der).unwrap();

            self.dal
                .update_field(
                    TeeDcapCollateralKind::SignCa,
                    hash.as_slice(),
                    not_after.to_system_time().into(),
                    &calldata,
                )
                .await?;
        }
        Ok(())
    }
}

fn get_next_update(tcbinfo_or_qe_identity_val: &Value) -> Result<DateTime<Utc>, TeeProcessorError> {
    let next_update = tcbinfo_or_qe_identity_val
        .get("nextUpdate")
        .context("Failed to get nextUpdate")?;
    let next_update = next_update.as_str().context("nextUpdate is not a string")?;
    let next_update =
        chrono::DateTime::parse_from_rfc3339(next_update).context("Failed to parse nextUpdate")?;
    Ok(next_update.to_utc())
}

fn extract_json_body(json_body: &str, body_element: &str) -> Result<String, TeeProcessorError> {
    let body_index = body_element.len() + 4;
    let body = json_body
        .split_at(body_index)
        .1
        .split(r#","signature":"#)
        .next()
        .ok_or(TeeProcessorError::GeneralError(format!(
            "Failed to extract {} from {}",
            body_element, json_body
        )))?;
    Ok(body.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_body() {
        let json_body = r#"{"enclaveIdentity":{"id":"QE","version":2,"issueDate":"2025-06-03T10:17:43Z","nextUpdate":"2025-07-03T10:17:43Z","tcbEvaluationDataNumber":17,"miscselect":"00000000","miscselectMask":"FFFFFFFF","attributes":"11000000000000000000000000000000","attributesMask":"FBFFFFFFFFFFFFFF0000000000000000","mrsigner":"8C4F5775D796503E96137F77C68A829A0056AC8DED70140B081B094490C57BFF","isvprodid":1,"tcbLevels":[{"tcb":{"isvsvn":8},"tcbDate":"2024-03-13T00:00:00Z","tcbStatus":"UpToDate"},{"tcb":{"isvsvn":6},"tcbDate":"2021-11-10T00:00:00Z","tcbStatus":"OutOfDate","advisoryIDs":["INTEL-SA-00615"]},{"tcb":{"isvsvn":5},"tcbDate":"2020-11-11T00:00:00Z","tcbStatus":"OutOfDate","advisoryIDs":["INTEL-SA-00477","INTEL-SA-00615"]},{"tcb":{"isvsvn":4},"tcbDate":"2019-11-13T00:00:00Z","tcbStatus":"OutOfDate","advisoryIDs":["INTEL-SA-00334","INTEL-SA-00477","INTEL-SA-00615"]},{"tcb":{"isvsvn":2},"tcbDate":"2019-05-15T00:00:00Z","tcbStatus":"OutOfDate","advisoryIDs":["INTEL-SA-00219","INTEL-SA-00293","INTEL-SA-00334","INTEL-SA-00477","INTEL-SA-00615"]},{"tcb":{"isvsvn":1},"tcbDate":"2018-08-15T00:00:00Z","tcbStatus":"OutOfDate","advisoryIDs":["INTEL-SA-00202","INTEL-SA-00219","INTEL-SA-00293","INTEL-SA-00334","INTEL-SA-00477","INTEL-SA-00615"]}]},"signature":"0f0387198364a37fe568df78e0939a19c899b9b573569d6bed95d8a27b26d3afe63a48e75128fed195f56ae31acf28bcc8a2369cf6238c110e13d087bf681697"}"#;
        let body = extract_json_body(json_body, "enclaveIdentity").unwrap();
        assert_eq!(
            body,
            r#"{"id":"QE","version":2,"issueDate":"2025-06-03T10:17:43Z","nextUpdate":"2025-07-03T10:17:43Z","tcbEvaluationDataNumber":17,"miscselect":"00000000","miscselectMask":"FFFFFFFF","attributes":"11000000000000000000000000000000","attributesMask":"FBFFFFFFFFFFFFFF0000000000000000","mrsigner":"8C4F5775D796503E96137F77C68A829A0056AC8DED70140B081B094490C57BFF","isvprodid":1,"tcbLevels":[{"tcb":{"isvsvn":8},"tcbDate":"2024-03-13T00:00:00Z","tcbStatus":"UpToDate"},{"tcb":{"isvsvn":6},"tcbDate":"2021-11-10T00:00:00Z","tcbStatus":"OutOfDate","advisoryIDs":["INTEL-SA-00615"]},{"tcb":{"isvsvn":5},"tcbDate":"2020-11-11T00:00:00Z","tcbStatus":"OutOfDate","advisoryIDs":["INTEL-SA-00477","INTEL-SA-00615"]},{"tcb":{"isvsvn":4},"tcbDate":"2019-11-13T00:00:00Z","tcbStatus":"OutOfDate","advisoryIDs":["INTEL-SA-00334","INTEL-SA-00477","INTEL-SA-00615"]},{"tcb":{"isvsvn":2},"tcbDate":"2019-05-15T00:00:00Z","tcbStatus":"OutOfDate","advisoryIDs":["INTEL-SA-00219","INTEL-SA-00293","INTEL-SA-00334","INTEL-SA-00477","INTEL-SA-00615"]},{"tcb":{"isvsvn":1},"tcbDate":"2018-08-15T00:00:00Z","tcbStatus":"OutOfDate","advisoryIDs":["INTEL-SA-00202","INTEL-SA-00219","INTEL-SA-00293","INTEL-SA-00334","INTEL-SA-00477","INTEL-SA-00615"]}]}"#
        );
    }
}

//! TEE contract interface

use zksync_contracts::tee_contract;
use zksync_l1_contract_interface::Tokenizable;
use zksync_types::{
    ethabi::{Bytes, Contract, Function, Token},
    H256, U256,
};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EnclaveId {
    QE = 0,
    QVE = 1,
    TDQE = 2,
}

impl From<EnclaveId> for u8 {
    fn from(id: EnclaveId) -> Self {
        id as u8
    }
}

impl TryFrom<&str> for EnclaveId {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.to_uppercase().as_str() {
            "QE" => Ok(Self::QE),
            "QVE" => Ok(Self::QVE),
            "TD_QE" => Ok(Self::TDQE),
            _ => Err(format!("Invalid enclave ID: {}", value)),
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CA {
    ROOT = 0,
    PROCESSOR = 1,
    PLATFORM = 2,
    SIGNING = 3,
}

impl From<CA> for u8 {
    fn from(ca: CA) -> Self {
        ca as u8
    }
}

pub(crate) fn get_function(contract: &Contract, name: &str) -> Function {
    contract
        .functions
        .get(name)
        .cloned()
        .unwrap_or_else(|| panic!("{} function not found", name))
        .pop()
        .unwrap_or_else(|| panic!("{} function entry not found", name))
}

#[derive(Debug, Clone)]
pub struct TeeFunctions {
    f_verify_and_attest_on_chain: Function,
    f_register_signer: Function,
    f_verify_digest: Function,
    f_upsert_root_certificate: Function,
    f_upsert_signing_certificate: Function,
    f_upsert_platform_certificate: Function,
    f_upsert_root_ca_crl: Function,
    f_upsert_pck_crl: Function,
    f_upsert_enclave_identity: Function,
    f_upsert_fmspc_tcb: Function,
}

impl Default for TeeFunctions {
    fn default() -> Self {
        let contract = tee_contract();

        Self {
            f_verify_and_attest_on_chain: get_function(&contract, "verifyAndAttestOnChain"),
            f_register_signer: get_function(&contract, "registerSigner"),
            f_verify_digest: get_function(&contract, "verifyDigest"),
            f_upsert_root_certificate: get_function(&contract, "upsertRootCertificate"),
            f_upsert_signing_certificate: get_function(&contract, "upsertSigningCertificate"),
            f_upsert_platform_certificate: get_function(&contract, "upsertPlatformCertificate"),
            f_upsert_root_ca_crl: get_function(&contract, "upsertRootCACrl"),
            f_upsert_pck_crl: get_function(&contract, "upsertPckCrl"),
            f_upsert_enclave_identity: get_function(&contract, "upsertEnclaveIdentity"),
            f_upsert_fmspc_tcb: get_function(&contract, "upsertFmspcTcb"),
        }
    }
}

impl TeeFunctions {
    /// function verifyAndAttestOnChain(bytes calldata rawQuote, bytes32 digest, bytes calldata signature) external {
    pub fn verify_and_attest_on_chain(
        &self,
        raw_quote: Vec<u8>,
        digest: H256,
        signature: Vec<u8>,
    ) -> zksync_types::ethabi::Result<Bytes> {
        self.f_verify_and_attest_on_chain.encode_input(&[
            raw_quote.into_token(),
            digest.into_token(),
            signature.into_token(),
        ])
    }

    /// function registerSigner(bytes calldata rawQuote) external {
    pub fn register_signer(&self, raw_quote: Vec<u8>) -> zksync_types::ethabi::Result<Bytes> {
        self.f_register_signer
            .encode_input(&[raw_quote.into_token()])
    }

    /// function verifyDigest(bytes32 digest, bytes calldata signature) external view {
    pub fn verify_digest(
        &self,
        digest: H256,
        signature: Vec<u8>,
    ) -> zksync_types::ethabi::Result<Bytes> {
        self.f_verify_digest
            .encode_input(&[digest.into_token(), signature.into_token()])
    }

    pub fn upsert_root_certificate(&self, cert: Vec<u8>) -> zksync_types::ethabi::Result<Bytes> {
        self.f_upsert_root_certificate
            .encode_input(&[cert.into_token()])
    }

    pub fn upsert_signing_certificate(&self, cert: Vec<u8>) -> zksync_types::ethabi::Result<Bytes> {
        self.f_upsert_signing_certificate
            .encode_input(&[cert.into_token()])
    }

    pub fn upsert_platform_certificate(
        &self,
        cert: Vec<u8>,
    ) -> zksync_types::ethabi::Result<Bytes> {
        self.f_upsert_platform_certificate
            .encode_input(&[cert.into_token()])
    }

    /// function upsertRootCACrl(bytes calldata rootcacrl) external returns (bytes32 attestationId){
    pub fn upsert_root_ca_crl(&self, root_ca_crl: Vec<u8>) -> zksync_types::ethabi::Result<Bytes> {
        self.f_upsert_root_ca_crl
            .encode_input(&[root_ca_crl.into_token()])
    }

    /// function upsertPckCrl(CA ca, bytes calldata crl) external returns (bytes32 attestationId){
    pub fn upsert_pck_crl(&self, ca: CA, crl: Vec<u8>) -> zksync_types::ethabi::Result<Bytes> {
        self.f_upsert_pck_crl
            .encode_input(&[U256::from(ca as u8).into_token(), crl.into_token()])
    }

    /// function upsertEnclaveIdentity(uint256 id, uint256 quote_version, EnclaveIdentityJsonObj calldata identityJson) external {
    pub fn upsert_enclave_identity(
        &self,
        id: EnclaveId,
        quote_version: u8,
        identity_json: String,
        signature: Vec<u8>,
    ) -> zksync_types::ethabi::Result<Bytes> {
        self.f_upsert_enclave_identity.encode_input(&[
            U256::from(id as u8).into_token(),
            U256::from(quote_version).into_token(),
            Token::Tuple(vec![Token::String(identity_json), signature.into_token()]),
        ])
    }

    /// function upsertFmspcTcb(TcbInfoJsonObj calldata tcbInfoJson) external {
    pub fn upsert_fmspc_tcb(
        &self,
        tcb_info_json: String,
        signature: Vec<u8>,
    ) -> zksync_types::ethabi::Result<Bytes> {
        self.f_upsert_fmspc_tcb.encode_input(&[Token::Tuple(vec![
            Token::String(tcb_info_json),
            signature.into_token(),
        ])])
    }
}

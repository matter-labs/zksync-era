//! TEE contract interface

use zksync_contracts::tee_contract;
use zksync_l1_contract_interface::Tokenizable;
use zksync_types::{
    ethabi::{Bytes, Function, Token},
    H256, U256,
};

use crate::zksync_functions::get_function;

#[derive(Debug)]
pub(super) struct TeeFunctions {
    f_verify_and_attest_on_chain: Function,
    f_upsert_pcs_certificates: Function,
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
            f_upsert_pcs_certificates: get_function(&contract, "upsertPcsCertificates"),
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

    /// function upsertPcsCertificates(CA[] calldata ca, bytes[] calldata certs) external returns (bytes32[] memory attestationIds){
    pub fn upsert_pcs_certificates(
        &self,
        ca: Vec<u8>,
        certs: Vec<u8>,
    ) -> zksync_types::ethabi::Result<Bytes> {
        self.f_upsert_pcs_certificates
            .encode_input(&[ca.into_token(), certs.into_token()])
    }

    /// function upsertRootCACrl(bytes calldata rootcacrl) external returns (bytes32 attestationId){
    pub fn upsert_root_ca_crl(&self, root_ca_crl: Vec<u8>) -> zksync_types::ethabi::Result<Bytes> {
        self.f_upsert_root_ca_crl
            .encode_input(&[root_ca_crl.into_token()])
    }

    /// function upsertPckCrl(CA ca, bytes calldata crl) external returns (bytes32 attestationId){
    pub fn upsert_pck_crl(&self, ca: Vec<u8>, crl: Vec<u8>) -> zksync_types::ethabi::Result<Bytes> {
        self.f_upsert_pck_crl
            .encode_input(&[ca.into_token(), crl.into_token()])
    }

    /// function upsertEnclaveIdentity(uint256 id, uint256 quoteVersion, EnclaveIdentityJsonObj calldata identityJson) external {
    pub fn upsert_enclave_identity(
        &self,
        id: U256,
        version: U256,
        identity_json: String,
        signature: Vec<u8>,
    ) -> zksync_types::ethabi::Result<Bytes> {
        self.f_upsert_enclave_identity.encode_input(&[
            id.into_token(),
            version.into_token(),
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

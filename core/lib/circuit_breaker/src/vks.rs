use crate::{utils::unwrap_tuple, CircuitBreaker, CircuitBreakerError};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::{env, str::FromStr};
use thiserror::Error;
use zksync_config::ZkSyncConfig;
use zksync_eth_client::clients::http_client::EthereumClient;
use zksync_types::ethabi::Token;
use zksync_types::zkevm_test_harness::bellman::{
    bn256::{Fq, Fq2, Fr, G1Affine, G2Affine},
    CurveAffine, PrimeField,
};
use zksync_types::{Address, H256};
use zksync_verification_key_server::get_vk_for_circuit_type;

#[derive(Debug, Error)]
pub enum VerifierError {
    #[error("Verifier address from the env var is different from the one in Diamond Proxy contract, from env: {address_from_env:?}, from contract: {address_from_contract:?}")]
    VerifierAddressMismatch {
        address_from_env: Address,
        address_from_contract: Address,
    },
    #[error("Server has different vks commitment from the one on L1 contract, server: {server_vks:?}, contract: {contract_vks:?}")]
    VksCommitment {
        server_vks: VksCommitment,
        contract_vks: VksCommitment,
    },
    #[error("Server has different Scheduler VK from the one on L1 contract, server: {server_vk}, contract: {contract_vk}")]
    SchedulerVk {
        server_vk: String,
        contract_vk: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VksCommitment {
    pub node: H256,
    pub leaf: H256,
    pub basic_circuits: H256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationKey {
    pub n: usize,
    pub num_inputs: usize,

    pub gate_setup_commitments: Vec<G1Affine>,
    pub gate_selectors_commitments: Vec<G1Affine>,
    pub permutation_commitments: Vec<G1Affine>,

    pub lookup_selector_commitment: Option<G1Affine>,
    pub lookup_tables_commitments: Vec<G1Affine>,
    pub lookup_table_type_commitment: Option<G1Affine>,

    pub non_residues: Vec<Fr>,
    pub g2_elements: [G2Affine; 2],
}

#[derive(Debug)]
pub struct VksChecker {
    pub eth_client: EthereumClient,
}

impl VksChecker {
    pub fn new(config: &ZkSyncConfig) -> Self {
        Self {
            eth_client: EthereumClient::from_config(config),
        }
    }

    async fn check_verifier_address(&self) -> Result<(), CircuitBreakerError> {
        let address_from_env =
            Address::from_str(&env::var("CONTRACTS_VERIFIER_ADDR").unwrap()).unwrap();
        let address_from_contract: Address = self
            .eth_client
            .call_main_contract_function("getVerifier", (), None, Default::default(), None)
            .await
            .unwrap();
        if address_from_env != address_from_contract {
            return Err(CircuitBreakerError::Verifier(
                VerifierError::VerifierAddressMismatch {
                    address_from_env,
                    address_from_contract,
                },
            ));
        }
        Ok(())
    }

    async fn check_commitments(&self) -> Result<(), CircuitBreakerError> {
        let verifier_params_token: Token = self
            .eth_client
            .call_main_contract_function("getVerifierParams", (), None, Default::default(), None)
            .await
            .unwrap();
        let vks_vec: Vec<H256> = unwrap_tuple(verifier_params_token)
            .into_iter()
            .map(|token| H256::from_slice(&token.into_fixed_bytes().unwrap()))
            .collect();
        let contract_vks = VksCommitment {
            node: vks_vec[0],
            leaf: vks_vec[1],
            basic_circuits: vks_vec[2],
        };

        let server_vks = VksCommitment {
            node: H256::from_str(&env::var("CONTRACTS_VK_COMMITMENT_NODE").unwrap()).unwrap(),
            leaf: H256::from_str(&env::var("CONTRACTS_VK_COMMITMENT_LEAF").unwrap()).unwrap(),
            basic_circuits: H256::from_str(
                &env::var("CONTRACTS_VK_COMMITMENT_BASIC_CIRCUITS").unwrap(),
            )
            .unwrap(),
        };

        if contract_vks != server_vks {
            return Err(CircuitBreakerError::Verifier(
                VerifierError::VksCommitment {
                    contract_vks,
                    server_vks,
                },
            ));
        }
        Ok(())
    }

    async fn check_scheduler_vk(&self) -> Result<(), CircuitBreakerError> {
        let server_vk = get_vk_for_circuit_type(0);
        let server_vk = VerificationKey {
            n: server_vk.n,
            num_inputs: server_vk.num_inputs,
            gate_setup_commitments: server_vk.gate_setup_commitments,
            gate_selectors_commitments: server_vk.gate_selectors_commitments,
            permutation_commitments: server_vk.permutation_commitments,
            lookup_selector_commitment: server_vk.lookup_selector_commitment,
            lookup_tables_commitments: server_vk.lookup_tables_commitments,
            lookup_table_type_commitment: server_vk.lookup_table_type_commitment,
            non_residues: server_vk.non_residues,
            g2_elements: server_vk.g2_elements,
        };

        let contract_vk = self.get_contract_vk().await;

        if server_vk != contract_vk {
            return Err(CircuitBreakerError::Verifier(VerifierError::SchedulerVk {
                server_vk: serde_json::to_string_pretty(&server_vk).unwrap(),
                contract_vk: serde_json::to_string_pretty(&contract_vk).unwrap(),
            }));
        }
        Ok(())
    }

    async fn get_contract_vk(&self) -> VerificationKey {
        let verifier_contract_address =
            Address::from_str(&env::var("CONTRACTS_VERIFIER_ADDR").unwrap()).unwrap();
        let verifier_contract_abi = zksync_contracts::verifier_contract();
        let vk_token: Token = self
            .eth_client
            .call_contract_function(
                "get_verification_key",
                (),
                None,
                Default::default(),
                None,
                verifier_contract_address,
                verifier_contract_abi,
            )
            .await
            .unwrap();

        let tokens = unwrap_tuple(vk_token);
        let n = tokens[0].clone().into_uint().unwrap().as_usize() - 1;
        let num_inputs = tokens[1].clone().into_uint().unwrap().as_usize();
        let gate_selectors_commitments = tokens[3]
            .clone()
            .into_fixed_array()
            .unwrap()
            .into_iter()
            .map(g1_affine_from_token)
            .collect();
        let gate_setup_commitments = tokens[4]
            .clone()
            .into_fixed_array()
            .unwrap()
            .into_iter()
            .map(g1_affine_from_token)
            .collect();
        let permutation_commitments = tokens[5]
            .clone()
            .into_fixed_array()
            .unwrap()
            .into_iter()
            .map(g1_affine_from_token)
            .collect();
        let lookup_selector_commitment = g1_affine_from_token(tokens[6].clone());
        let lookup_tables_commitments = tokens[7]
            .clone()
            .into_fixed_array()
            .unwrap()
            .into_iter()
            .map(g1_affine_from_token)
            .collect();
        let lookup_table_type_commitment = g1_affine_from_token(tokens[8].clone());
        let non_residues = tokens[9]
            .clone()
            .into_fixed_array()
            .unwrap()
            .into_iter()
            .map(fr_from_token)
            .collect();
        let g2_elements = tokens[10]
            .clone()
            .into_fixed_array()
            .unwrap()
            .into_iter()
            .map(g2_affine_from_token)
            .collect::<Vec<G2Affine>>()
            .try_into()
            .unwrap();

        VerificationKey {
            n,
            num_inputs,

            gate_setup_commitments,
            gate_selectors_commitments,
            permutation_commitments,

            lookup_selector_commitment: Some(lookup_selector_commitment),
            lookup_tables_commitments,
            lookup_table_type_commitment: Some(lookup_table_type_commitment),

            non_residues,
            g2_elements,
        }
    }
}

#[async_trait::async_trait]
impl CircuitBreaker for VksChecker {
    async fn check(&self) -> Result<(), CircuitBreakerError> {
        self.check_verifier_address().await?;
        self.check_commitments().await?;
        self.check_scheduler_vk().await?;
        Ok(())
    }
}

fn g1_affine_from_token(token: Token) -> G1Affine {
    let tokens = unwrap_tuple(token);
    G1Affine::from_xy_unchecked(
        Fq::from_str(&tokens[0].clone().into_uint().unwrap().to_string()).unwrap(),
        Fq::from_str(&tokens[1].clone().into_uint().unwrap().to_string()).unwrap(),
    )
}

fn fr_from_token(token: Token) -> Fr {
    let tokens = unwrap_tuple(token);
    Fr::from_str(&tokens[0].clone().into_uint().unwrap().to_string()).unwrap()
}

fn g2_affine_from_token(token: Token) -> G2Affine {
    let tokens = unwrap_tuple(token);
    let tokens0 = tokens[0].clone().into_fixed_array().unwrap();
    let tokens1 = tokens[1].clone().into_fixed_array().unwrap();
    G2Affine::from_xy_unchecked(
        Fq2 {
            c1: Fq::from_str(&tokens0[0].clone().into_uint().unwrap().to_string()).unwrap(),
            c0: Fq::from_str(&tokens0[1].clone().into_uint().unwrap().to_string()).unwrap(),
        },
        Fq2 {
            c1: Fq::from_str(&tokens1[0].clone().into_uint().unwrap().to_string()).unwrap(),
            c0: Fq::from_str(&tokens1[1].clone().into_uint().unwrap().to_string()).unwrap(),
        },
    )
}

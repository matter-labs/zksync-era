use crate::{utils::unwrap_tuple, CircuitBreaker, CircuitBreakerError};

use std::collections::BTreeMap;
use std::env;
use std::fmt;
use std::str::FromStr;
use std::{fs, path::Path};

use convert_case::{Case, Casing};

use zksync_config::ZkSyncConfig;
use zksync_eth_client::clients::http_client::EthereumClient;
use zksync_types::ethabi::Token;
use zksync_types::Address;

#[derive(Debug)]
pub struct MismatchedFacetSelectorsError {
    pub server_selectors: String,
    pub contract_selectors: String,
}

impl fmt::Display for MismatchedFacetSelectorsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "server: {}, contract: {}",
            self.server_selectors, self.contract_selectors
        )
    }
}

#[derive(Debug)]
pub struct FacetSelectorsChecker {
    eth_client: EthereumClient,
    // BTreeMap is used to have fixed order of elements when printing error.
    server_selectors: BTreeMap<Address, Vec<String>>,
}

impl FacetSelectorsChecker {
    pub fn new(config: &ZkSyncConfig) -> Self {
        let zksync_home = env::var("ZKSYNC_HOME").unwrap_or_else(|_| ".".into());
        let path_str = "contracts/ethereum/artifacts/cache/solpp-generated-contracts/zksync/facets";
        let facets_path = Path::new(&zksync_home).join(path_str);
        let paths = fs::read_dir(facets_path).unwrap();
        let server_selectors = paths
            .into_iter()
            .filter_map(|path| {
                let file_name: String = path.unwrap().file_name().into_string().unwrap();
                let facet_name: &str = file_name.as_str().split('.').next().unwrap();
                // Exclude `Base` contract.
                if facet_name == "Base" {
                    return None;
                }
                let env_name = format!(
                    "CONTRACTS_{}_FACET_ADDR",
                    facet_name.to_case(Case::ScreamingSnake)
                );
                let address = Address::from_str(&env::var(env_name).unwrap()).unwrap();

                let contract = zksync_contracts::load_contract(
                    format!("{0}/{1}.sol/{1}Facet.json", path_str, facet_name).as_str(),
                );
                let selectors = contract
                    .functions
                    .into_values()
                    .map(|func| {
                        let func = func.first().cloned().unwrap();
                        format!("0x{}", hex::encode(func.short_signature()))
                    })
                    .collect();

                Some((address, selectors))
            })
            .collect();

        Self {
            eth_client: EthereumClient::from_config(config),
            server_selectors,
        }
    }
}

impl FacetSelectorsChecker {
    async fn get_contract_facet_selectors(&self) -> BTreeMap<Address, Vec<String>> {
        let facets: Token = self
            .eth_client
            .call_main_contract_function("facets", (), None, Default::default(), None)
            .await
            .unwrap();
        let facets = facets.into_array().unwrap();
        facets
            .into_iter()
            .map(|facet| {
                let tokens = unwrap_tuple(facet);
                let address = tokens[0].clone().into_address().unwrap();
                let selectors = tokens[1]
                    .clone()
                    .into_array()
                    .unwrap()
                    .into_iter()
                    .map(|token| {
                        "0x".to_string() + hex::encode(token.into_fixed_bytes().unwrap()).as_str()
                    })
                    .collect();
                (address, selectors)
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl CircuitBreaker for FacetSelectorsChecker {
    async fn check(&self) -> Result<(), CircuitBreakerError> {
        let contract_selectors = self.get_contract_facet_selectors().await;
        if self.server_selectors != contract_selectors {
            return Err(CircuitBreakerError::MismatchedFacetSelectors(
                MismatchedFacetSelectorsError {
                    server_selectors: serde_json::to_string_pretty(&self.server_selectors).unwrap(),
                    contract_selectors: serde_json::to_string_pretty(&contract_selectors).unwrap(),
                },
            ));
        }

        Ok(())
    }
}

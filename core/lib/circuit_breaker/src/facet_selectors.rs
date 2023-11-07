use backon::{ConstantBuilder, Retryable};
use convert_case::{Case, Casing};
use std::{collections::BTreeMap, env, fmt, fs, path::Path, str::FromStr};

use zksync_config::configs::chain::CircuitBreakerConfig;
use zksync_contracts::zksync_contract;
use zksync_eth_client::{types::Error as EthClientError, EthInterface};
use zksync_types::{ethabi::Token, Address, H160};

// local imports
use crate::{utils::unwrap_tuple, CircuitBreaker, CircuitBreakerError};

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
pub struct FacetSelectorsChecker<E> {
    eth_client: E,
    // BTreeMap is used to have fixed order of elements when printing error.
    server_selectors: BTreeMap<Address, Vec<String>>,
    config: CircuitBreakerConfig,
    main_contract: H160,
}

impl<E: EthInterface + std::fmt::Debug> FacetSelectorsChecker<E> {
    pub fn new(config: &CircuitBreakerConfig, eth_client: E, main_contract: H160) -> Self {
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
                if facet_name == "Base" || facet_name.contains("validium") {
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
                // Filter out `getName` function. Because it's a part of the common interface and it could messed up the selectors
                let selectors = contract
                    .functions
                    .into_values()
                    .filter(|func| {
                        let func = func.first().cloned().unwrap();
                        func.name != "getName"
                    })
                    .map(|func| {
                        let func = func.first().cloned().unwrap();
                        format!("0x{}", hex::encode(func.short_signature()))
                    })
                    .collect();

                Some((address, selectors))
            })
            .collect();

        Self {
            eth_client,
            server_selectors,
            config: config.clone(),
            main_contract,
        }
    }
}

impl<E: EthInterface + std::fmt::Debug> FacetSelectorsChecker<E> {
    async fn get_contract_facet_selectors(&self) -> BTreeMap<Address, Vec<String>> {
        let facets = self.get_facets_token_with_retry().await.unwrap();

        parse_faucets_token(facets)
    }

    pub(super) async fn get_facets_token_with_retry(&self) -> Result<Token, EthClientError> {
        (|| async {
            let result: Result<Token, EthClientError> = self
                .eth_client
                .call_contract_function(
                    "facets",
                    (),
                    None,
                    Default::default(),
                    None,
                    self.main_contract,
                    zksync_contract(),
                )
                .await;

            result
        })
        .retry(
            &ConstantBuilder::default()
                .with_max_times(self.config.http_req_max_retry_number)
                .with_delay(self.config.http_req_retry_interval()),
        )
        .await
    }
}

#[async_trait::async_trait]
impl<E: EthInterface + std::fmt::Debug> CircuitBreaker for FacetSelectorsChecker<E> {
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

fn parse_faucets_token(facets: Token) -> BTreeMap<Address, Vec<String>> {
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

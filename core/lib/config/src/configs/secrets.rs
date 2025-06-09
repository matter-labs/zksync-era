// TODO: remove non-secret / secret config split (isn't necessary)

use anyhow::Context;
use smart_config::{
    de::{FromSecretString, Optional, Serde},
    DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::{secrets::APIKey, url::SensitiveUrl};

use crate::configs::{
    consensus::ConsensusSecrets,
    da_client::{avail::AvailSecrets, celestia::CelestiaSecrets, eigen::EigenSecrets},
};

const EXAMPLE_POSTGRES_URL: &str = "postgres://postgres:notsecurepassword@localhost:5432/zksync";

#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct DatabaseSecrets {
    /// Postgres connection string for the server database.
    #[config(alias = "url", secret, with = Optional(Serde![str]))]
    #[config(example = Some(EXAMPLE_POSTGRES_URL.parse().unwrap()))]
    pub server_url: Option<SensitiveUrl>,
    /// Postgres connection string for the prover database. Not used by external nodes.
    #[config(secret, with = Optional(Serde![str]))]
    pub prover_url: Option<SensitiveUrl>,
    /// Postgres connection string for the server replica (readonly).
    #[config(alias = "replica_url", secret, with = Optional(Serde![str]))]
    #[config(example = Some(EXAMPLE_POSTGRES_URL.parse().unwrap()))]
    pub server_replica_url: Option<SensitiveUrl>,
}

#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct L1Secrets {
    /// Web3 RPC URL for L1 (e.g., Ethereum mainnet).
    #[config(alias = "web3_url", secret, with = Optional(Serde![str]))]
    #[config(example = Some("https://ethereum-rpc.publicnode.com/".parse().unwrap()))]
    pub l1_rpc_url: Option<SensitiveUrl>,
    /// Web3 RPC URL for the gateway layer.
    #[config(alias = "gateway_web3_url", secret, with = Optional(Serde![str]))]
    pub gateway_rpc_url: Option<SensitiveUrl>,
}

#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
#[config(tag = "client")]
pub enum DataAvailabilitySecrets {
    Avail(AvailSecrets),
    Celestia(CelestiaSecrets),
    Eigen(EigenSecrets),
    // Needed for compatibility with the non-secret part of the DA config
    NoDA,
}

#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct ContractVerifierSecrets {
    /// Etherscan API key that is used for contract verification in Etherscan.
    /// If not set, the Etherscan verification is disabled.
    #[config(with = Optional(FromSecretString))]
    pub etherscan_api_key: Option<APIKey>,
}

#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
pub struct Secrets {
    #[config(nest)]
    pub consensus: ConsensusSecrets,
    #[config(nest)]
    pub database: DatabaseSecrets,
    #[config(alias = "eth_client", nest)]
    pub l1: L1Secrets,
    #[config(nest, rename = "da_client", alias = "da")]
    pub data_availability: Option<DataAvailabilitySecrets>,
    #[config(nest)]
    pub contract_verifier: ContractVerifierSecrets,
}

impl DatabaseSecrets {
    /// Returns a copy of the master database URL as a `Result` to simplify error propagation.
    pub fn master_url(&self) -> anyhow::Result<SensitiveUrl> {
        self.server_url.clone().context("Master DB URL is absent")
    }

    /// Returns a copy of the replica database URL as a `Result` to simplify error propagation.
    pub fn replica_url(&self) -> anyhow::Result<SensitiveUrl> {
        if let Some(replica_url) = &self.server_replica_url {
            Ok(replica_url.clone())
        } else {
            self.master_url()
        }
    }

    /// Returns a copy of the prover database URL as a `Result` to simplify error propagation.
    pub fn prover_url(&self) -> anyhow::Result<SensitiveUrl> {
        self.prover_url.clone().context("Prover DB URL is absent")
    }
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;
    use smart_config::{testing::test_complete, Environment, Yaml};

    use super::*;

    fn assert_secrets(secrets: Secrets) {
        assert_eq!(
            secrets.database.server_url.unwrap().expose_str(),
            "postgres://postgres:notsecurepassword@localhost:5432/zksync_server_localhost_era"
        );
        assert_eq!(
            secrets.database.server_replica_url.unwrap().expose_str(),
            "postgres://postgres:notsecurepassword@localhost/zksync_replica_local"
        );
        assert_eq!(
            secrets.database.prover_url.unwrap().expose_str(),
            "postgres://postgres:notsecurepassword@localhost/prover_local"
        );
        assert_eq!(
            secrets.l1.l1_rpc_url.unwrap().expose_str(),
            "http://127.0.0.1:8545/"
        );
        assert_eq!(
            secrets.l1.gateway_rpc_url.unwrap().expose_str(),
            "http://127.0.0.1:4050/"
        );
        assert_eq!(
            secrets.consensus.validator_key.unwrap().expose_secret(),
            "validator:secret:bls12_381:2e78025015c2b4ba44b081d404c5446442dac74d5a20334c90af90a0b9987866"
        );
        assert_eq!(
            secrets.consensus.node_key.unwrap().expose_secret(),
            "node:secret:ed25519:d1aaab7e5bc33cce10418d832a43b6aa00f67f2499d48a62fe79a190f1d6b0a3"
        );

        let DataAvailabilitySecrets::Avail(avail) = secrets.data_availability.unwrap() else {
            panic!("unexpected DA secrets");
        };
        assert_eq!(
            avail.seed_phrase.unwrap().0.expose_secret(),
            "correct horse battery staple"
        );
        assert_eq!(
            avail.gas_relay_api_key.unwrap().0.expose_secret(),
            "SUPER_SECRET"
        );
    }

    // Migration path: change `DA_SECRETS_*` -> `DA_*`
    #[test]
    fn parsing_from_env() {
        let env = r#"
            DATABASE_URL=postgres://postgres:notsecurepassword@localhost:5432/zksync_server_localhost_era
            DATABASE_REPLICA_URL=postgres://postgres:notsecurepassword@localhost/zksync_replica_local
            DATABASE_PROVER_URL=postgres://postgres:notsecurepassword@localhost/prover_local

            ETH_CLIENT_WEB3_URL=http://127.0.0.1:8545/
            ETH_CLIENT_GATEWAY_WEB3_URL=http://127.0.0.1:4050/

            DA_CLIENT="Avail"
            DA_SEED_PHRASE="correct horse battery staple"
            DA_GAS_RELAY_API_KEY="SUPER_SECRET"

            CONTRACT_VERIFIER_ETHERSCAN_API_KEY=correct horse battery staple

            CONSENSUS_VALIDATOR_KEY="validator:secret:bls12_381:2e78025015c2b4ba44b081d404c5446442dac74d5a20334c90af90a0b9987866"
            CONSENSUS_NODE_KEY="node:secret:ed25519:d1aaab7e5bc33cce10418d832a43b6aa00f67f2499d48a62fe79a190f1d6b0a3"
        "#;
        let env = Environment::from_dotenv("test.env", env).unwrap();
        let secrets: Secrets = test_complete(env).unwrap();
        assert_secrets(secrets);
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
            database:
              server_url: postgres://postgres:notsecurepassword@localhost:5432/zksync_server_localhost_era
              server_replica_url: postgres://postgres:notsecurepassword@localhost/zksync_replica_local
              prover_url: postgres://postgres:notsecurepassword@localhost/prover_local
            l1:
              l1_rpc_url: http://127.0.0.1:8545/
              gateway_rpc_url: http://127.0.0.1:4050/
            consensus:
              validator_key: validator:secret:bls12_381:2e78025015c2b4ba44b081d404c5446442dac74d5a20334c90af90a0b9987866
              node_key: node:secret:ed25519:d1aaab7e5bc33cce10418d832a43b6aa00f67f2499d48a62fe79a190f1d6b0a3
            contract_verifier:
              etherscan_api_key: null
            da:
              client: Avail
              seed_phrase: 'correct horse battery staple'
              gas_relay_api_key: SUPER_SECRET
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let secrets: Secrets = test_complete(yaml).unwrap();
        assert_secrets(secrets);
    }
}

use anyhow::Context;
use smart_config::{
    de::{Optional, Serde},
    DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::url::SensitiveUrl;

use crate::configs::{
    consensus::ConsensusSecrets,
    da_client::{avail::AvailSecrets, celestia::CelestiaSecrets, eigen::EigenSecrets},
};

#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct DatabaseSecrets {
    /// Postgres connection string for the server database.
    #[config(alias = "url", secret, with = Optional(Serde![str]))]
    pub server_url: Option<SensitiveUrl>,
    /// Postgres connection string for the prover database.
    #[config(secret, with = Optional(Serde![str]))]
    pub prover_url: Option<SensitiveUrl>,
    /// Postgres connection string for the server replica (readonly).
    #[config(alias = "replica_url", secret, with = Optional(Serde![str]))]
    pub server_replica_url: Option<SensitiveUrl>,
}

#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct L1Secrets {
    /// RPC URL for L1.
    #[config(alias = "web3_url", secret, with = Optional(Serde![str]))]
    pub l1_rpc_url: Option<SensitiveUrl>,
}

#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
#[config(tag = "client")]
pub enum DataAvailabilitySecrets {
    Avail(AvailSecrets),
    Celestia(CelestiaSecrets),
    Eigen(EigenSecrets),
}

#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
pub struct Secrets {
    #[config(nest)]
    pub consensus: ConsensusSecrets,
    #[config(nest)]
    pub database: DatabaseSecrets,
    #[config(alias = "eth_client", nest)]
    pub l1: L1Secrets,
    #[config(rename = "da", nest)]
    pub data_availability: Option<DataAvailabilitySecrets>,
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
    use smart_config::{testing::test, Environment, Yaml};

    use super::*;

    fn assert_secrets(secrets: Secrets) {
        assert_eq!(
            secrets.database.server_url.unwrap().expose_str(),
            "postgres://postgres:notsecurepassword@localhost:5432/zksync_server_localhost_era"
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
            secrets.consensus.validator_key.unwrap().expose_secret(),
            "validator:secret:bls12_381:2e78025015c2b4ba44b081d404c5446442dac74d5a20334c90af90a0b9987866"
        );
        assert_eq!(
            secrets.consensus.node_key.unwrap().expose_secret(),
            "node:secret:ed25519:d1aaab7e5bc33cce10418d832a43b6aa00f67f2499d48a62fe79a190f1d6b0a3"
        );
        assert_eq!(
            secrets.consensus.attester_key.unwrap().expose_secret(),
            "attester:secret:secp256k1:a32fc914d5131642cf7d7952d95e018c95b1852eafb4204043c4a9614488de2e"
        );
        let DataAvailabilitySecrets::Avail(avail) = secrets.data_availability.unwrap() else {
            panic!("unexpected DA secrets");
        };
        assert_eq!(
            avail.seed_phrase.0.expose_secret(),
            "correct horse battery staple"
        );
        assert_eq!(avail.gas_relay_api_key.0.expose_secret(), "SUPER_SECRET");
    }

    // Migration path: shorten `DA_SECRETS_*` -> `DA_*`.
    #[test]
    fn parsing_from_env() {
        let env = r#"
            DATABASE_URL=postgres://postgres:notsecurepassword@localhost:5432/zksync_server_localhost_era
            DATABASE_REPLICA_URL=postgres://postgres:notsecurepassword@localhost/zksync_replica_local
            DATABASE_PROVER_URL=postgres://postgres:notsecurepassword@localhost/prover_local

            ETH_CLIENT_WEB3_URL=http://127.0.0.1:8545/

            DA_CLIENT="Avail"
            DA_SEED_PHRASE="correct horse battery staple"
            DA_GAS_RELAY_API_KEY="SUPER_SECRET"

            CONSENSUS_VALIDATOR_KEY="validator:secret:bls12_381:2e78025015c2b4ba44b081d404c5446442dac74d5a20334c90af90a0b9987866"
            CONSENSUS_NODE_KEY="node:secret:ed25519:d1aaab7e5bc33cce10418d832a43b6aa00f67f2499d48a62fe79a190f1d6b0a3"
            CONSENSUS_ATTESTER_KEY="attester:secret:secp256k1:a32fc914d5131642cf7d7952d95e018c95b1852eafb4204043c4a9614488de2e"
        "#;
        let env = Environment::from_dotenv("test.env", env).unwrap();
        let secrets: Secrets = test(env).unwrap();
        assert_secrets(secrets);
    }

    // Migration path: use tagged enum for DA secrets (are they used anywhere?)
    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
            database:
              server_url: postgres://postgres:notsecurepassword@localhost:5432/zksync_server_localhost_era
              prover_url: postgres://postgres:notsecurepassword@localhost/prover_local
            l1:
              l1_rpc_url: http://127.0.0.1:8545/
            consensus:
              validator_key: validator:secret:bls12_381:2e78025015c2b4ba44b081d404c5446442dac74d5a20334c90af90a0b9987866
              node_key: node:secret:ed25519:d1aaab7e5bc33cce10418d832a43b6aa00f67f2499d48a62fe79a190f1d6b0a3
              attester_key: attester:secret:secp256k1:a32fc914d5131642cf7d7952d95e018c95b1852eafb4204043c4a9614488de2e
            da:
              client: Avail
              seed_phrase: 'correct horse battery staple'
              gas_relay_api_key: SUPER_SECRET
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let secrets: Secrets = test(yaml).unwrap();
        assert_secrets(secrets);
    }
}

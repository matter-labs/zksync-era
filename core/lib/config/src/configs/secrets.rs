// TODO: remove non-secret / secret config split (isn't necessary)

use anyhow::Context;
use smart_config::{de::Serde, fallback, DescribeConfig, DeserializeConfig};
use zksync_basic_types::url::SensitiveUrl;

const EXAMPLE_POSTGRES_URL: &str = "postgres://postgres:notsecurepassword@localhost:5432/zksync";

#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct PostgresSecrets {
    /// Postgres connection string for the server database.
    #[config(alias = "url", secret, with = Serde![str])]
    #[config(fallback = &fallback::Env("DATABASE_URL"))]
    #[config(example = Some(EXAMPLE_POSTGRES_URL.parse().unwrap()))]
    pub server_url: Option<SensitiveUrl>,
    /// Postgres connection string for the prover database. Not used by external nodes.
    #[config(secret, with = Serde![str])]
    pub prover_url: Option<SensitiveUrl>,
    /// Postgres connection string for the server replica (readonly).
    #[config(alias = "replica_url", secret, with = Serde![str])]
    #[config(example = Some(EXAMPLE_POSTGRES_URL.parse().unwrap()))]
    pub server_replica_url: Option<SensitiveUrl>,
}

#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct L1Secrets {
    /// RPC URL for L1.
    #[config(alias = "eth_client_url", secret, with = Serde![str])]
    #[config(example = Some("https://ethereum-rpc.publicnode.com/".parse().unwrap()))]
    pub l1_rpc_url: Option<SensitiveUrl>,
    /// Web3 RPC URL for the gateway layer.
    #[config(secret, with = Serde![str])]
    #[config(alias = "gateway_web3_url", alias = "gateway_url")]
    pub gateway_rpc_url: Option<SensitiveUrl>,
}

#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
pub struct Secrets {
    #[config(nest, alias = "database")]
    pub postgres: PostgresSecrets,
    #[config(nest)]
    pub l1: L1Secrets,
}

impl PostgresSecrets {
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
    use smart_config::{testing::test_complete, Environment, Yaml};

    use super::*;

    fn assert_secrets(secrets: Secrets) {
        assert_eq!(
            secrets.postgres.server_url.unwrap().expose_str(),
            "postgres://postgres:notsecurepassword@localhost:5432/zksync_server_localhost_era"
        );
        assert_eq!(
            secrets.postgres.server_replica_url.unwrap().expose_str(),
            "postgres://postgres:notsecurepassword@localhost/zksync_replica_local"
        );
        assert_eq!(
            secrets.postgres.prover_url.unwrap().expose_str(),
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
    }

    // Migration path: change `DA_SECRETS_*` -> `DA_*`
    #[test]
    fn parsing_from_env() {
        let env = r#"
            DATABASE_URL=postgres://postgres:notsecurepassword@localhost:5432/zksync_server_localhost_era
            DATABASE_REPLICA_URL=postgres://postgres:notsecurepassword@localhost/zksync_replica_local
            DATABASE_PROVER_URL=postgres://postgres:notsecurepassword@localhost/prover_local

            # Was `ETH_CLIENT_WEB3_URL`
            L1_ETH_CLIENT_URL=http://127.0.0.1:8545/
            # Was `ETH_CLIENT_GATEWAY_WEB3_URL`
            L1_GATEWAY_WEB3_URL=http://127.0.0.1:4050/
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
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let secrets: Secrets = test_complete(yaml).unwrap();
        assert_secrets(secrets);
    }
}

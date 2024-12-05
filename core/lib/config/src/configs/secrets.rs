use anyhow::Context;
use zksync_basic_types::url::SensitiveUrl;

use crate::configs::{
    consensus::ConsensusSecrets,
    da_client::{avail::AvailSecrets, celestia::CelestiaSecrets, eigen::EigenSecrets},
};

#[derive(Debug, Clone, PartialEq)]
pub struct DatabaseSecrets {
    pub server_url: Option<SensitiveUrl>,
    pub prover_url: Option<SensitiveUrl>,
    pub server_replica_url: Option<SensitiveUrl>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct L1Secrets {
    pub l1_rpc_url: SensitiveUrl,
    pub gateway_rpc_url: Option<SensitiveUrl>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DataAvailabilitySecrets {
    Avail(AvailSecrets),
    Celestia(CelestiaSecrets),
    Eigen(EigenSecrets),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Secrets {
    pub consensus: Option<ConsensusSecrets>,
    pub database: Option<DatabaseSecrets>,
    pub l1: Option<L1Secrets>,
    pub data_availability: Option<DataAvailabilitySecrets>,
}

impl Secrets {
    pub fn prover_url_host(&self) -> Option<String> {
        self.database
            .as_ref()
            .and_then(|database| database.prover_url.as_ref())
            .and_then(|url| url.expose_url().host_str())
            .map(String::from)
    }

    pub fn set_prover_url_host(&mut self, host: &str) -> anyhow::Result<()> {
        if let Some(database) = self.database.as_mut() {
            if let Some(url) = database.prover_url.as_mut() {
                let mut new_url = url.expose_url().clone();
                new_url.set_host(Some(host))?;
                *url = SensitiveUrl::from(new_url);
            }
        }

        Ok(())
    }

    pub fn server_url_host(&self) -> Option<String> {
        self.database
            .as_ref()
            .and_then(|database| database.server_url.as_ref())
            .and_then(|url| url.expose_url().host_str())
            .map(String::from)
    }

    pub fn set_server_url_host(&mut self, host: &str) -> anyhow::Result<()> {
        if let Some(database) = self.database.as_mut() {
            if let Some(url) = database.server_url.as_mut() {
                let mut new_url = url.expose_url().clone();
                new_url.set_host(Some(host))?;
                *url = SensitiveUrl::from(new_url);
            }
        }

        Ok(())
    }

    pub fn l1_rpc_url_host(&self) -> Option<String> {
        self.l1
            .as_ref()
            .and_then(|l1| l1.l1_rpc_url.expose_url().host_str())
            .map(String::from)
    }

    pub fn set_l1_rpc_url_host(&mut self, host: &str) -> anyhow::Result<()> {
        if let Some(l1) = &mut self.l1 {
            let mut new_url = l1.l1_rpc_url.expose_url().clone();
            new_url.set_host(Some(host))?;
            l1.l1_rpc_url = SensitiveUrl::from(new_url);
        }

        Ok(())
    }
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

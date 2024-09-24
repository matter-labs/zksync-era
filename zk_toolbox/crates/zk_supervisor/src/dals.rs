use anyhow::{anyhow, Context};
use common::config::global_config;
use config::{EcosystemConfig, SecretsConfig};
use url::Url;
use xshell::Shell;

use crate::{
    commands::database::args::DalUrls,
    messages::{MSG_CHAIN_NOT_FOUND_ERR, MSG_DATABASE_MUST_BE_PRESENTED},
};

pub const CORE_DAL_PATH: &str = "core/lib/dal";
pub const PROVER_DAL_PATH: &str = "prover/crates/lib/prover_dal";

#[derive(Debug, Clone)]
pub struct SelectedDals {
    pub prover: bool,
    pub core: bool,
}

impl SelectedDals {
    /// Returns true if no databases are selected
    pub fn none(&self) -> bool {
        !self.prover && !self.core
    }
}

#[derive(Debug, Clone)]
pub struct Dal {
    pub path: String,
    pub url: Url,
}

pub fn get_dals(
    shell: &Shell,
    selected_dals: &SelectedDals,
    urls: &DalUrls,
) -> anyhow::Result<Vec<Dal>> {
    let mut dals = vec![];

    if selected_dals.prover {
        dals.push(get_prover_dal(shell, urls.prover.clone())?);
    }
    if selected_dals.core {
        dals.push(get_core_dal(shell, urls.core.clone())?);
    }

    Ok(dals)
}

pub fn get_prover_dal(shell: &Shell, url: Option<String>) -> anyhow::Result<Dal> {
    let url = if let Some(url) = url {
        Url::parse(&url)?
    } else {
        let secrets = get_secrets(shell)?;
        secrets
            .database
            .as_ref()
            .context(MSG_DATABASE_MUST_BE_PRESENTED)?
            .prover_url()?
            .expose_url()
            .clone()
    };

    Ok(Dal {
        path: PROVER_DAL_PATH.to_string(),
        url,
    })
}

pub fn get_core_dal(shell: &Shell, url: Option<String>) -> anyhow::Result<Dal> {
    let url = if let Some(url) = url {
        Url::parse(&url)?
    } else {
        let secrets = get_secrets(shell)?;
        secrets
            .database
            .as_ref()
            .context(MSG_DATABASE_MUST_BE_PRESENTED)?
            .master_url()?
            .expose_url()
            .clone()
    };

    Ok(Dal {
        path: CORE_DAL_PATH.to_string(),
        url,
    })
}

fn get_secrets(shell: &Shell) -> anyhow::Result<SecretsConfig> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(global_config().chain_name.clone())
        .ok_or(anyhow!(MSG_CHAIN_NOT_FOUND_ERR))?;
    let secrets = chain_config.get_secrets_config()?;

    Ok(secrets)
}

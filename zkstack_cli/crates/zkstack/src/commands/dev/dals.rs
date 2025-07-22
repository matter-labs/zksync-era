use anyhow::Context as _;
use url::Url;
use xshell::Shell;
use zkstack_cli_config::{EcosystemConfig, SecretsConfig};

use super::{commands::database::args::DalUrls, messages::MSG_CHAIN_NOT_FOUND_ERR};

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

pub async fn get_dals(
    shell: &Shell,
    selected_dals: &SelectedDals,
    urls: &DalUrls,
) -> anyhow::Result<Vec<Dal>> {
    let mut dals = vec![];

    if selected_dals.prover {
        dals.push(get_prover_dal(shell, urls.prover.clone()).await?);
    }
    if selected_dals.core {
        dals.push(get_core_dal(shell, urls.core.clone()).await?);
    }

    Ok(dals)
}

pub async fn get_prover_dal(shell: &Shell, url: Option<String>) -> anyhow::Result<Dal> {
    let url = if let Some(url) = url {
        Url::parse(&url)?
    } else {
        let secrets = get_secrets(shell).await?;
        secrets
            .prover_database_url()?
            .context("missing prover database URL")?
    };

    Ok(Dal {
        path: PROVER_DAL_PATH.to_string(),
        url,
    })
}

pub async fn get_core_dal(shell: &Shell, url: Option<String>) -> anyhow::Result<Dal> {
    let url = if let Some(url) = url {
        Url::parse(&url)?
    } else {
        let secrets = get_secrets(shell).await?;
        secrets
            .core_database_url()?
            .context("missing core database URL")?
    };

    Ok(Dal {
        path: CORE_DAL_PATH.to_string(),
        url,
    })
}

async fn get_secrets(shell: &Shell) -> anyhow::Result<SecretsConfig> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    chain_config.get_secrets_config().await
}

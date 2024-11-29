use common::{github::GitHubTagFetcher, server::ExecutionMode, PromptSelect};
use config::{traits::SaveConfigWithBasePath, ChainConfig};
use xshell::Shell;

use crate::messages::MSG_SERVER_SELECT_DOCKER_IMAGE_TAG;

pub async fn select_tag() -> anyhow::Result<String> {
    let fetcher = GitHubTagFetcher::new(None)?;
    let gh_tags = fetcher.get_newest_core_tags(Some(5)).await?;

    let tags: Vec<String> = std::iter::once("latest".to_string())
        .chain(
            gh_tags
                .iter()
                .map(|r| r.name.trim_start_matches("core-").to_string()),
        )
        .collect();

    Ok(PromptSelect::new(MSG_SERVER_SELECT_DOCKER_IMAGE_TAG, tags).ask())
}

pub fn adjust_host_to_execution_mode(
    shell: &Shell,
    mode: &ExecutionMode,
    chain_config: &ChainConfig,
) -> anyhow::Result<()> {
    let mut secrets = chain_config.get_secrets_config()?;
    let mut general = chain_config.get_general_config()?;

    match mode {
        ExecutionMode::Release | ExecutionMode::Debug => {
            general.set_prometheus_host("localhost")?;
            secrets.set_prover_url_host("localhost")?;
            secrets.set_server_url_host("localhost")?;
            secrets.set_l1_rpc_url_host("localhost")?;
        }
        ExecutionMode::Docker { tag: _ } => {
            general.set_prometheus_host("host.docker.internal")?;
            secrets.set_prover_url_host("host.docker.internal")?;
            secrets.set_server_url_host("host.docker.internal")?;
            secrets.set_l1_rpc_url_host("host.docker.internal")?;
        }
    }

    secrets.save_with_base_path(shell, &chain_config.configs)?;
    general.save_with_base_path(shell, &chain_config.configs)?;

    Ok(())
}

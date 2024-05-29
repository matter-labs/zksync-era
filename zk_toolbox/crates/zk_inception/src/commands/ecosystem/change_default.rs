use common::PromptSelect;
use xshell::Shell;

use crate::{
    commands::ecosystem::args::change_default::ChangeDefaultChain,
    configs::{EcosystemConfig, SaveConfig},
    consts::CONFIG_NAME,
};

pub fn run(
    args: ChangeDefaultChain,
    shell: &Shell,
    mut ecosystem_config: EcosystemConfig,
) -> anyhow::Result<()> {
    let chains = ecosystem_config.list_of_chains();
    let chain_name = args.name.unwrap_or_else(|| {
        PromptSelect::new("What chain you want to set as default?", &chains)
            .ask()
            .to_string()
    });

    if !chains.contains(&chain_name) {
        anyhow::bail!(
            "Chain with name {} doesnt exist, please choose one of {:?}",
            chain_name,
            &chains
        );
    }
    ecosystem_config.default_chain = chain_name;
    ecosystem_config.save(shell, CONFIG_NAME)
}

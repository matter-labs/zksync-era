use xshell::Shell;
use zkstack_cli_common::PromptSelect;
use zkstack_cli_config::{traits::SaveConfigWithBasePath, ZkStackConfig};

use crate::{
    commands::ecosystem::args::change_default::ChangeDefaultChain,
    messages::{msg_chain_doesnt_exist_err, MSG_DEFAULT_CHAIN_PROMPT},
};

pub fn run(args: ChangeDefaultChain, shell: &Shell) -> anyhow::Result<()> {
    let mut ecosystem_config = ZkStackConfig::ecosystem(shell)?;

    let chains = ecosystem_config.list_of_chains();
    let chain_name = args.name.unwrap_or_else(|| {
        PromptSelect::new(MSG_DEFAULT_CHAIN_PROMPT, &chains)
            .ask()
            .to_string()
    });

    if !chains.contains(&chain_name) {
        anyhow::bail!(msg_chain_doesnt_exist_err(&chain_name, &chains));
    }
    ecosystem_config.set_default_chain(chain_name);
    ecosystem_config.save_with_base_path(shell, ".")
}

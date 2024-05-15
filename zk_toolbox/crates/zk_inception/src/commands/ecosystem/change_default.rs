use common::PromptSelect;
use xshell::Shell;

use crate::{
    commands::ecosystem::args::change_default::ChangeDefaultHyperchain,
    configs::{EcosystemConfig, SaveConfig},
    consts::CONFIG_NAME,
};

pub fn run(args: ChangeDefaultHyperchain, shell: &Shell) -> anyhow::Result<()> {
    let mut ecosystem_config = EcosystemConfig::from_file()?;

    let hyperchains = ecosystem_config.list_of_hyperchains();
    let hyperchain_name = args.name.unwrap_or_else(|| {
        PromptSelect::new("What hyperchain you want to set as default?", &hyperchains)
            .ask()
            .to_string()
    });

    if !hyperchains.contains(&hyperchain_name) {
        anyhow::bail!(
            "Hyperchain with name {} doesnt exist, please choose one of {:?}",
            hyperchain_name,
            &hyperchains
        );
    }
    ecosystem_config.default_hyperchain = hyperchain_name;
    ecosystem_config.save(shell, CONFIG_NAME)
}

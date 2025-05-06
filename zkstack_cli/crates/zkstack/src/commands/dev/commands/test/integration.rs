use std::{path::PathBuf, time::Duration};

use anyhow::Context;
use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, config::global_config, logger};
use zkstack_cli_config::EcosystemConfig;

use super::{
    args::integration::IntegrationArgs,
    utils::{
        build_contracts, install_and_build_dependencies, TestWallets, TEST_WALLETS_PATH,
        TS_INTEGRATION_PATH,
    },
};
use crate::commands::dev::messages::{
    msg_integration_tests_run, MSG_CHAIN_NOT_FOUND_ERR, MSG_DESERIALIZE_TEST_WALLETS_ERR,
    MSG_INTEGRATION_TESTS_RUN_SUCCESS,
};

#[derive(Debug)]
pub(super) struct IntegrationTestRunner<'a> {
    shell: &'a Shell,
    no_deps: bool,
    ecosystem_config: EcosystemConfig,
    test_timeout: Duration,
    test_suites: Vec<&'a str>,
    test_pattern: Option<&'a str>,
}

impl<'a> IntegrationTestRunner<'a> {
    pub fn new(shell: &'a Shell, no_deps: bool) -> anyhow::Result<Self> {
        let ecosystem_config = EcosystemConfig::from_file(shell)?;
        Ok(Self {
            shell,
            no_deps,
            ecosystem_config,
            test_timeout: Duration::from_secs(240),
            test_suites: vec![],
            test_pattern: None,
        })
    }

    pub fn current_chain(&self) -> &str {
        self.ecosystem_config.current_chain()
    }

    pub fn with_test_suite(mut self, name: &'a str) -> Self {
        self.test_suites.push(name);
        self
    }

    fn with_test_suites(mut self, names: impl Iterator<Item = &'a str>) -> Self {
        self.test_suites.extend(names);
        self
    }

    pub fn with_test_pattern(mut self, pattern: Option<&'a str>) -> Self {
        self.test_pattern = pattern;
        self
    }

    pub async fn build_command(self) -> anyhow::Result<xshell::Cmd<'a>> {
        let ecosystem_config = self.ecosystem_config;
        let chain_config = ecosystem_config
            .load_current_chain()
            .context(MSG_CHAIN_NOT_FOUND_ERR)?;
        self.shell
            .change_dir(ecosystem_config.link_to_code.join(TS_INTEGRATION_PATH));

        if !self.no_deps {
            install_and_build_dependencies(self.shell, &ecosystem_config)?;
            build_contracts(self.shell, &ecosystem_config)?;
        }

        let wallets_path: PathBuf = ecosystem_config.link_to_code.join(TEST_WALLETS_PATH);
        let raw_wallets = self.shell.read_file(&wallets_path)?;
        let wallets: TestWallets =
            serde_json::from_str(&raw_wallets).context(MSG_DESERIALIZE_TEST_WALLETS_ERR)?;

        wallets
            .init_test_wallet(&ecosystem_config, &chain_config)
            .await?;

        let test_pattern: &[_] = if let Some(pattern) = self.test_pattern {
            &["-t", pattern]
        } else {
            &[]
        };
        let test_suites = if self.test_suites.is_empty() {
            None
        } else {
            let names = self.test_suites.join("|");
            Some(format!("/({names}).test.ts"))
        };
        let timeout_ms = self.test_timeout.as_millis().to_string();
        let mut command = cmd!(
            self.shell,
            "yarn jest --forceExit --testTimeout {timeout_ms} {test_pattern...} {test_suites...}"
        )
        .env("CHAIN_NAME", ecosystem_config.current_chain())
        .env("MASTER_WALLET_PK", wallets.get_test_pk(&chain_config)?);

        if global_config().verbose {
            command = command.env(
                "ZKSYNC_DEBUG_LOGS",
                format!("{:?}", global_config().verbose),
            );
        }
        Ok(command)
    }
}

pub async fn run(shell: &Shell, args: IntegrationArgs) -> anyhow::Result<()> {
    logger::info(msg_integration_tests_run(args.external_node));
    let mut command = IntegrationTestRunner::new(shell, args.no_deps)?
        .with_test_suites(args.suite.iter().map(String::as_str))
        .with_test_pattern(args.test_pattern.as_deref())
        .build_command()
        .await?;
    if args.external_node {
        command = command.env("EXTERNAL_NODE", format!("{:?}", args.external_node))
    }
    if args.evm {
        command = command.env("RUN_EVM_TEST", "1");
    }

    Cmd::new(command).with_force_run().run()?;
    logger::outro(MSG_INTEGRATION_TESTS_RUN_SUCCESS);
    Ok(())
}

use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use clap::{Parser, ValueEnum};
use ethers::{
    core::types::Bytes,
    middleware::Middleware,
    prelude::{LocalWallet, Signer},
    types::{Address, H256, U256},
    utils::{hex, hex::ToHexExt},
};
use serde::{Deserialize, Serialize};
use strum::Display;
use url::Url;
use xshell::Shell;

use super::runner::ForgeRunner;
use crate::{docker::adjust_localhost_for_docker, ethereum::create_ethers_client};

/// ForgeScript is a wrapper around the forge script command.
pub struct ForgeScript {
    pub(crate) base_path: PathBuf,
    pub(crate) script_path: PathBuf,
    pub(crate) args: ForgeScriptArgs,
}

impl ForgeScript {
    /// Run the forge script command using default runner configuration.
    pub fn run(self, shell: &Shell) -> anyhow::Result<()> {
        let mut runner = ForgeRunner::default();
        runner.run(shell, self)
    }

    pub fn wallet_args_passed(&self) -> bool {
        self.args.wallet_args_passed()
    }

    /// Add the ffi flag to the forge script command.
    pub fn with_ffi(mut self) -> Self {
        self.args.add_arg(ForgeScriptArg::Ffi);
        self
    }

    /// Add the sender address to the forge script command.
    pub fn with_sender(mut self, address: String) -> Self {
        self.args.add_arg(ForgeScriptArg::Sender { address });
        self
    }

    /// Add the rpc-url flag to the forge script command.
    pub fn with_rpc_url(mut self, rpc_url: String) -> Self {
        self.args.add_arg(ForgeScriptArg::RpcUrl { url: rpc_url });
        self
    }

    /// Add the broadcast flag to the forge script command.
    pub fn with_broadcast(mut self) -> Self {
        self.args.add_arg(ForgeScriptArg::Broadcast);
        self
    }

    pub fn with_signature(mut self, signature: &str) -> Self {
        self.args.add_arg(ForgeScriptArg::Sig {
            sig: signature.to_string(),
        });
        self
    }

    pub fn with_zksync(mut self) -> Self {
        self.args.add_arg(ForgeScriptArg::Zksync);
        self
    }

    pub fn with_calldata(mut self, calldata: &Bytes) -> Self {
        self.args.add_arg(ForgeScriptArg::Sig {
            sig: hex::encode(calldata),
        });
        self
    }

    pub fn with_gas_limit(mut self, gas_limit: u64) -> Self {
        self.args.add_arg(ForgeScriptArg::GasLimit { gas_limit });
        self
    }

    /// Makes sure a transaction is sent, only after its previous one has been confirmed and succeeded.
    pub fn with_slow(mut self) -> Self {
        self.args.add_arg(ForgeScriptArg::Slow);
        self
    }

    /// Adds the private key of the deployer account.
    pub fn with_private_key(mut self, private_key: H256) -> Self {
        self.args.add_arg(ForgeScriptArg::PrivateKey {
            private_key: private_key.encode_hex(),
        });
        self
    }

    // Do not start the script if balance is not enough
    pub fn private_key(&self) -> Option<LocalWallet> {
        self.args.args.iter().find_map(|a| {
            if let ForgeScriptArg::PrivateKey { private_key } = a {
                let key = H256::from_str(private_key).unwrap();
                let key = LocalWallet::from_bytes(key.as_bytes()).unwrap();
                Some(key)
            } else {
                None
            }
        })
    }

    pub fn rpc_url(&self) -> Option<String> {
        self.args.args.iter().find_map(|a| {
            if let ForgeScriptArg::RpcUrl { url } = a {
                Some(url.clone())
            } else {
                None
            }
        })
    }

    pub(crate) fn sig(&self) -> Option<String> {
        self.args.args.iter().find_map(|a| {
            if let ForgeScriptArg::Sig { sig } = a {
                Some(sig.clone())
            } else {
                None
            }
        })
    }

    pub fn address(&self) -> Option<Address> {
        self.private_key().map(|k| k.address())
    }

    pub async fn get_the_balance(&self) -> anyhow::Result<Option<U256>> {
        let Some(rpc_url) = self.rpc_url() else {
            return Ok(None);
        };
        let Some(private_key) = self.private_key() else {
            return Ok(None);
        };
        let client = create_ethers_client(private_key, rpc_url, None)?;
        let balance = client.get_balance(client.address(), None).await?;
        Ok(Some(balance))
    }

    pub(crate) fn needs_bridgehub_skip(&self) -> bool {
        self.script_path == Path::new("deploy-scripts/DeployCTM.s.sol")
    }

    pub(crate) fn script_name(&self) -> &Path {
        &self.script_path
    }

    pub(crate) fn base_path(&self) -> &Path {
        &self.base_path
    }
}

const PROHIBITED_ARGS: [&str; 10] = [
    "--contracts",
    "--root",
    "--lib-paths",
    "--out",
    "--sig",
    "--target-contract",
    "--chain-id",
    "-C",
    "-O",
    "-s",
];

const WALLET_ARGS: [&str; 18] = [
    "-a",
    "--froms",
    "-i",
    "--private-keys",
    "--private-key",
    "--mnemonics",
    "--mnenomic-passphrases",
    "--mnemonic-derivation-paths",
    "--mnemonic-indexes",
    "--keystore",
    "--account",
    "--password",
    "--password-file",
    "-l",
    "--ledger",
    "-t",
    "--trezor",
    "--aws",
];

/// Set of known forge script arguments necessary for execution.
#[derive(Display, Debug, Serialize, Deserialize, Clone, PartialEq)]
#[strum(serialize_all = "kebab-case", prefix = "--")]
pub enum ForgeScriptArg {
    Broadcast,
    #[strum(to_string = "etherscan-api-key={api_key}")]
    EtherscanApiKey {
        api_key: String,
    },
    Ffi,
    #[strum(to_string = "private-key={private_key}")]
    PrivateKey {
        private_key: String,
    },
    #[strum(to_string = "rpc-url={url}")]
    RpcUrl {
        url: String,
    },
    #[strum(to_string = "sig={sig}")]
    Sig {
        sig: String,
    },
    Slow,
    #[strum(to_string = "verifier={verifier}")]
    Verifier {
        verifier: String,
    },
    #[strum(to_string = "verifier-url={url}")]
    VerifierUrl {
        url: String,
    },
    Verify,
    Resume,
    #[strum(to_string = "sender={address}")]
    Sender {
        address: String,
    },
    #[strum(to_string = "gas-limit={gas_limit}")]
    GasLimit {
        gas_limit: u64,
    },
    Zksync,
    #[strum(to_string = "skip={skip_path}")]
    Skip {
        skip_path: String,
    },
}

/// ForgeScriptArgs is a set of arguments that can be passed to the forge script command.
#[derive(Default, Debug, Serialize, Deserialize, Parser, Clone)]
#[clap(next_help_heading = "Forge options")]
pub struct ForgeScriptArgs {
    /// List of known forge script arguments.
    #[clap(skip)]
    pub(crate) args: Vec<ForgeScriptArg>,
    /// Verify deployed contracts
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub verify: Option<bool>,
    /// Verifier to use
    #[clap(long, default_value_t = ForgeVerifier::Etherscan)]
    pub verifier: ForgeVerifier,
    /// Verifier URL, if using a custom provider
    #[clap(long)]
    pub verifier_url: Option<String>,
    /// Verifier API key
    #[clap(long)]
    pub verifier_api_key: Option<String>,
    #[clap(long)]
    pub resume: bool,
    #[clap(long)]
    pub zksync: bool,
    /// List of additional arguments that can be passed through the CLI.
    ///
    /// e.g.: `zkstack init -a --private-key=<PRIVATE_KEY>`
    #[clap(long, short)]
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = false)]
    pub(crate) additional_args: Vec<String>,
}

impl ForgeScriptArgs {
    /// Build the forge script command arguments for a specific runner mode.
    pub(crate) fn build_for_runner(&mut self, use_docker: bool) -> Vec<String> {
        self.add_verify_args();
        self.cleanup_contract_args();
        if use_docker {
            self.adjust_args_for_docker();
        }
        if self.zksync {
            self.add_arg(ForgeScriptArg::Zksync);
        }

        self.args
            .iter()
            .map(|arg| arg.to_string())
            .chain(self.additional_args.clone())
            .collect()
    }

    /// Builds the forge script command arguments without any runner customisation.
    pub fn build(&mut self) -> Vec<String> {
        self.build_for_runner(false)
    }

    /// Adds verify arguments to the forge script command.
    fn add_verify_args(&mut self) {
        if !self.verify.is_some_and(|v| v) {
            return;
        }

        self.add_arg(ForgeScriptArg::Verify);
        if let Some(url) = &self.verifier_url {
            self.add_arg(ForgeScriptArg::VerifierUrl { url: url.clone() });
        }
        if let Some(api_key) = &self.verifier_api_key {
            self.add_arg(ForgeScriptArg::EtherscanApiKey {
                api_key: api_key.clone(),
            });
        }
        self.add_arg(ForgeScriptArg::Verifier {
            verifier: self.verifier.to_string(),
        });
    }

    /// Cleanup the contract arguments which are not allowed to be passed through the CLI.
    fn cleanup_contract_args(&mut self) {
        let mut skip_next = false;
        let mut cleaned_args = vec![];
        let mut forbidden_args = vec![];

        let prohibited_with_spacing: Vec<String> = PROHIBITED_ARGS
            .iter()
            .flat_map(|arg| vec![format!("{arg} "), format!("{arg}\t")])
            .collect();

        let prohibited_with_equals: Vec<String> = PROHIBITED_ARGS
            .iter()
            .map(|arg| format!("{arg}="))
            .collect();

        for arg in self.additional_args.iter() {
            if skip_next {
                skip_next = false;
                continue;
            }

            if prohibited_with_spacing
                .iter()
                .any(|prohibited_arg| arg.starts_with(prohibited_arg))
            {
                skip_next = true;
                forbidden_args.push(arg.clone());
                continue;
            }

            if prohibited_with_equals
                .iter()
                .any(|prohibited_arg| arg.starts_with(prohibited_arg))
            {
                skip_next = false;
                forbidden_args.push(arg.clone());
                continue;
            }

            cleaned_args.push(arg.clone());
        }

        if !forbidden_args.is_empty() {
            println!(
                "Warning: the following arguments are not allowed to be passed through the CLI and were skipped: {:?}",
                forbidden_args
            );
        }

        self.additional_args = cleaned_args;
    }

    // If running via Docker, rewrite localhost RPCs to host.docker.internal
    fn adjust_args_for_docker(&mut self) {
        if let Some(ForgeScriptArg::RpcUrl { url }) = self
            .args
            .iter_mut()
            .find(|a| matches!(a, ForgeScriptArg::RpcUrl { .. }))
        {
            if let Ok(parsed) = Url::parse(url) {
                if let Ok(new_url) = adjust_localhost_for_docker(parsed) {
                    *url = new_url.to_string();
                }
            }
        }
    }

    /// Add additional arguments to the forge script command.
    /// If the argument already exists, a warning will be printed.
    pub fn add_arg(&mut self, arg: ForgeScriptArg) {
        if self.args.contains(&arg) {
            println!("Warning: argument {arg:?} already exists");
            return;
        }
        self.args.push(arg);
    }

    pub fn wallet_args_passed(&self) -> bool {
        self.additional_args
            .iter()
            .any(|arg| WALLET_ARGS.contains(&arg.as_ref()))
    }

    pub fn with_zksync(&mut self) {
        self.zksync = true;
    }
}

#[derive(Debug, Clone, ValueEnum, Display, Serialize, Deserialize, Default)]
#[strum(serialize_all = "snake_case")]
pub enum ForgeVerifier {
    #[default]
    Etherscan,
    Sourcify,
    Blockscout,
    Oklink,
}

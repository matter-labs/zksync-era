use std::path::{Path, PathBuf};
use std::str::FromStr;

use clap::Parser;
use ethers::abi::Address;
use ethers::middleware::Middleware;
use ethers::prelude::{LocalWallet, Signer, U256};
use ethers::{abi::AbiEncode, types::H256};
use serde::{Deserialize, Serialize};
use strum_macros::Display;
use xshell::{cmd, Shell};

use crate::cmd::Cmd;
use crate::ethereum::create_ethers_client;

/// Forge is a wrapper around the forge binary.
pub struct Forge {
    path: PathBuf,
}

impl Forge {
    /// Create a new Forge instance.
    pub fn new(path: &Path) -> Self {
        Forge {
            path: path.to_path_buf(),
        }
    }

    /// Create a new ForgeScript instance.
    ///
    /// The script path can be passed as a relative path to the base path
    /// or as an absolute path.
    pub fn script(&self, path: &Path, args: ForgeScriptArgs) -> ForgeScript {
        ForgeScript {
            base_path: self.path.clone(),
            script_path: path.to_path_buf(),
            args,
        }
    }
}

/// ForgeScript is a wrapper around the forge script command.
pub struct ForgeScript {
    base_path: PathBuf,
    script_path: PathBuf,
    args: ForgeScriptArgs,
}

impl ForgeScript {
    /// Run the forge script command.
    pub fn run(mut self, shell: &Shell) -> anyhow::Result<()> {
        let _dir_guard = shell.push_dir(&self.base_path);
        let script_path = self.script_path.as_os_str();
        let args = self.args.build();
        Cmd::new(cmd!(shell, "forge script {script_path} --legacy {args...}")).run()?;
        Ok(())
    }

    pub fn wallet_args_passed(&self) -> bool {
        self.args.wallet_args_passed()
    }

    /// Add the ffi flag to the forge script command.
    pub fn with_ffi(mut self) -> Self {
        self.args.add_arg(ForgeScriptArg::Ffi);
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
    pub fn private_key(&self) -> Option<H256> {
        self.args.args.iter().find_map(|a| {
            if let ForgeScriptArg::PrivateKey { private_key } = a {
                Some(H256::from_str(private_key).unwrap())
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

    pub fn address(&self) -> Option<Address> {
        self.private_key().and_then(|a| {
            LocalWallet::from_bytes(a.as_bytes())
                .ok()
                .map(|a| a.address())
        })
    }

    pub async fn check_the_balance(&self, minimum_value: U256) -> anyhow::Result<bool> {
        let Some(rpc_url) = self.rpc_url() else {
            return Ok(true);
        };
        let Some(private_key) = self.private_key() else {
            return Ok(true);
        };
        let client = create_ethers_client(private_key, rpc_url)?;
        let balance = client.get_balance(client.address(), None).await?;
        Ok(balance > minimum_value)
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
    Ffi,
    #[strum(to_string = "rpc-url={url}")]
    RpcUrl {
        url: String,
    },
    Broadcast,
    Slow,
    #[strum(to_string = "private-key={private_key}")]
    PrivateKey {
        private_key: String,
    },
    #[strum(to_string = "sig={sig}")]
    Sig {
        sig: String,
    },
}

/// ForgeScriptArgs is a set of arguments that can be passed to the forge script command.
#[derive(Default, Debug, Serialize, Deserialize, Parser, Clone)]
pub struct ForgeScriptArgs {
    /// List of known forge script arguments.
    #[clap(skip)]
    args: Vec<ForgeScriptArg>,
    /// List of additional arguments that can be passed through the CLI.
    ///
    /// e.g.: `zk_inception init -a --private-key=<PRIVATE_KEY>`
    #[clap(long, short, help_heading = "Forge options")]
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = false)]
    additional_args: Vec<String>,
}

impl ForgeScriptArgs {
    /// Build the forge script command arguments.
    pub fn build(&mut self) -> Vec<String> {
        self.cleanup_contract_args();
        self.args
            .iter()
            .map(|arg| arg.to_string())
            .chain(self.additional_args.clone())
            .collect()
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
}

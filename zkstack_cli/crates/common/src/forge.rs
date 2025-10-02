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
use xshell::{cmd, Shell};

use crate::{
    cmd::{Cmd, CmdResult},
    ethereum::create_ethers_client,
};

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
        // When running the DeployCTM script, we skip recompiling the Bridgehub
        // because it must be compiled with a low optimizer-runs value.
        if self.script_path == Path::new("deploy-scripts/DeployCTM.s.sol") {
            let skip_path: String = String::from("contracts/bridgehub/*");
            self.args.add_arg(ForgeScriptArg::Skip { skip_path });
        }
        let _dir_guard = shell.push_dir(&self.base_path);
        let script_path = self.script_path.as_os_str();
        let args_no_resume = self.args.build();
        if self.args.resume {
            let mut args = args_no_resume.clone();
            args.push(ForgeScriptArg::Resume.to_string());
            let res = Cmd::new(cmd!(shell, "forge script {script_path} --legacy {args...}"))
                .with_piped_std_err()
                .run();
            if !res.resume_not_successful_because_has_not_began() {
                return Ok(res?);
            }
        }

        // TODO: This line is very helpful for debugging purposes,
        // maybe it makes sense to make it conditionally displayed.
        let command = format!(
            "forge script {} --legacy {}",
            script_path.to_str().unwrap(),
            args_no_resume.join(" ")
        );

        println!("Command: {}", command);

        let mut cmd = Cmd::new(cmd!(
            shell,
            "forge script {script_path} --legacy {args_no_resume...}"
        ));

        if self.args.resume {
            cmd = cmd.with_piped_std_err();
        }

        let res = cmd.run();
        // We won't catch this error if resume is not set.
        if res.proposal_error() {
            return Ok(());
        }
        Ok(res?)
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
    args: Vec<ForgeScriptArg>,
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
    additional_args: Vec<String>,
}

impl ForgeScriptArgs {
    /// Build the forge script command arguments.
    pub fn build(&mut self) -> Vec<String> {
        self.add_verify_args();
        self.cleanup_contract_args();
        if self.zksync {
            self.add_arg(ForgeScriptArg::Zksync);
        }
        self.args
            .iter()
            .map(|arg| arg.to_string())
            .chain(self.additional_args.clone())
            .collect()
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

// Trait for handling forge errors. Required for implementing method for CmdResult
trait ForgeErrorHandler {
    // Resume doesn't work if the forge script has never been started on this chain before.
    // So we want to catch it and try again without resume arg if it's the case
    fn resume_not_successful_because_has_not_began(&self) -> bool;
    // Catch the error if upgrade tx has already been processed. We do execute much of
    // txs using upgrade mechanism and if this particular upgrade has already been processed we could assume
    // it as a success
    fn proposal_error(&self) -> bool;
}

impl ForgeErrorHandler for CmdResult<()> {
    fn resume_not_successful_because_has_not_began(&self) -> bool {
        let text = "Deployment not found for chain";
        check_error(self, text)
    }

    fn proposal_error(&self) -> bool {
        let text = "revert: Operation with this proposal id already exists";
        check_error(self, text)
    }
}

fn check_error(cmd_result: &CmdResult<()>, error_text: &str) -> bool {
    if let Err(cmd_error) = &cmd_result {
        if let Some(stderr) = &cmd_error.stderr {
            return stderr.contains(error_text);
        }
    }
    false
}

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use ethers::contract::Abigen;
use xshell::{cmd, Shell};

const COMPLETION_DIR: &str = "completion";

fn main() -> anyhow::Result<()> {
    let outdir = PathBuf::from(std::env::var("OUT_DIR")?).canonicalize()?;
    Abigen::new("ConsensusRegistry", "abi/ConsensusRegistry.json")
        .map_err(|_| anyhow!("Failed ABI deserialization"))?
        .generate()
        .map_err(|_| anyhow!("Failed ABI generation"))?
        .write_to_file(outdir.join("consensus_registry_abi.rs"))
        .context("Failed to write ABI to file")?;

    if let Err(e) = build_dependencies() {
        println!("cargo:error=It was not possible to install projects dependencies");
        println!("cargo:error={}", e);
    }

    if let Err(e) = configure_shell_autocompletion() {
        println!("cargo:warning=It was not possible to install autocomplete scripts. Please generate them manually with `zkstack autocomplete`");
        println!("cargo:error={}", e);
    };

    Ok(())
}

fn configure_shell_autocompletion() -> anyhow::Result<()> {
    // Array of supported shells
    let shells = [
        clap_complete::Shell::Bash,
        clap_complete::Shell::Fish,
        clap_complete::Shell::Zsh,
    ];

    for shell in shells {
        std::fs::create_dir_all(&shell.autocomplete_folder()?)
            .context("it was impossible to create the configuration directory")?;

        let src = Path::new(COMPLETION_DIR).join(shell.autocomplete_file_name()?);
        let dst = shell
            .autocomplete_folder()?
            .join(shell.autocomplete_file_name()?);

        std::fs::copy(src, dst)?;

        shell
            .configure_autocomplete()
            .context("failed to run extra configuration requirements")?;
    }

    Ok(())
}

pub trait ShellAutocomplete {
    fn autocomplete_folder(&self) -> anyhow::Result<PathBuf>;
    fn autocomplete_file_name(&self) -> anyhow::Result<String>;
    /// Extra steps required for shells enable command autocomplete.
    fn configure_autocomplete(&self) -> anyhow::Result<()>;
}

impl ShellAutocomplete for clap_complete::Shell {
    fn autocomplete_folder(&self) -> anyhow::Result<PathBuf> {
        let home_dir = dirs::home_dir().context("missing home folder")?;

        match self {
            clap_complete::Shell::Bash => Ok(home_dir.join(".bash_completion.d")),
            clap_complete::Shell::Fish => Ok(home_dir.join(".config/fish/completions")),
            clap_complete::Shell::Zsh => Ok(home_dir.join(".zsh/completion")),
            _ => anyhow::bail!("unsupported shell"),
        }
    }

    fn autocomplete_file_name(&self) -> anyhow::Result<String> {
        let crate_name = env!("CARGO_PKG_NAME");

        match self {
            clap_complete::Shell::Bash => Ok(format!("{}.sh", crate_name)),
            clap_complete::Shell::Fish => Ok(format!("{}.fish", crate_name)),
            clap_complete::Shell::Zsh => Ok(format!("_{}.zsh", crate_name)),
            _ => anyhow::bail!("unsupported shell"),
        }
    }

    fn configure_autocomplete(&self) -> anyhow::Result<()> {
        match self {
            clap_complete::Shell::Bash | clap_complete::Shell::Zsh => {
                let shell = &self.to_string().to_lowercase();
                let completion_file = self
                    .autocomplete_folder()?
                    .join(self.autocomplete_file_name()?);

                // Source the completion file inside .{shell}rc
                let shell_rc = dirs::home_dir()
                    .context("missing home directory")?
                    .join(format!(".{}rc", shell));

                if shell_rc.exists() {
                    let shell_rc_content = std::fs::read_to_string(&shell_rc)
                        .context(format!("could not read .{}rc", shell))?;

                    if !shell_rc_content.contains("# zkstack completion") {
                        let completion_snippet = if shell == "zsh" {
                            format!(
                                "{}\n# zkstack completion\nautoload -Uz compinit\ncompinit\nsource \"{}\"\n",
                                shell_rc_content,
                                completion_file.to_str().unwrap()
                            )
                        } else {
                            format!(
                                "{}\n# zkstack completion\nsource \"{}\"\n",
                                shell_rc_content,
                                completion_file.to_str().unwrap()
                            )
                        };

                        std::fs::write(shell_rc, completion_snippet)
                            .context(format!("could not write .{}rc", shell))?;
                    }
                } else {
                    println!(
                        "cargo:warning=Please add the following line to your .{}rc:",
                        shell
                    );
                    println!("cargo:warning=source {}", completion_file.to_str().unwrap());
                }
            }
            _ => (),
        }

        Ok(())
    }
}

fn build_dependencies() -> anyhow::Result<()> {
    let shell = Shell::new()?;
    let code_dir = Path::new("../");

    let _dir_guard = shell.push_dir(code_dir);

    cmd!(shell, "yarn install")
        .run()
        .context("Failed to install dependencies")
}

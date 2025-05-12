//! CLI extensions for configs.

use clap::{Args, Subcommand, ValueEnum};
use smart_config::{ConfigRepository, SerializerOptions};

#[derive(Debug, Args)]
struct HelpCommandArgs {
    /// Filter for the full param name.
    filter: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SerializationFormat {
    Json,
    Yaml,
}

#[derive(Debug, Subcommand)]
#[command(disable_help_subcommand = true)] // we define our own `help` subcommand
enum ConfigCommand {
    /// Outputs help for the provided params.
    Help(HelpCommandArgs),
    /// Debugs config param values.
    Debug {
        /// Filter for the full param name.
        filter: Option<String>,
    },
    /// Outputs the parsed config params back in the machine-readable format (e.g., YAML).
    Print {
        /// Serialization format to use.
        #[arg(long, value_enum, default_value_t = SerializationFormat::Yaml)]
        format: SerializationFormat,
        /// Only output the diff with default param values.
        #[arg(long)]
        diff: bool,
    },
}

impl ConfigCommand {
    fn run(self, repo: ConfigRepository<'_>) -> anyhow::Result<()> {
        let mut printer = smart_config_commands::Printer::stdout();
        match self {
            Self::Help(HelpCommandArgs { filter }) => {
                printer.print_help(repo.schema(), |param| {
                    filter
                        .as_ref()
                        .is_none_or(|needle| param.all_paths().any(|path| path.contains(needle)))
                })?;
            }
            Self::Debug { filter } => {
                printer.print_debug(&repo, |param| {
                    filter
                        .as_ref()
                        .is_none_or(|needle| param.all_paths().any(|path| path.contains(needle)))
                })??;
            }
            Self::Print { diff, format } => {
                let options = if diff {
                    SerializerOptions::diff_with_default()
                } else {
                    SerializerOptions::default()
                };

                let json = serde_json::Value::from(repo.canonicalize(&options)?);
                match format {
                    SerializationFormat::Yaml => printer.print_yaml(&json)?,
                    SerializationFormat::Json => printer.print_json(&json)?,
                }
            }
        }
        Ok(())
    }
}

/// `clap` arguments for a `config` subcommand that can be integrated into a binary.
#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
#[command(flatten_help = true)]
pub struct ConfigArgs {
    #[command(subcommand)]
    command: Option<ConfigCommand>,
    #[command(flatten)]
    args: HelpCommandArgs,
}

impl ConfigArgs {
    /// Runs this command. By convention, the binary should exit after executing the command.
    pub fn run(self, repo: ConfigRepository<'_>) -> anyhow::Result<()> {
        let command = self.command.unwrap_or(ConfigCommand::Help(self.args));
        command.run(repo)
    }
}

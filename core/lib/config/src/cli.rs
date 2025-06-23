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
    /// Environment vars in the YAML format. Can be added to the `environment` section in Docker Compose spec etc.
    EnvYaml,
}

impl SerializationFormat {
    fn is_flat(self) -> bool {
        matches!(self, Self::EnvYaml)
    }
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
    fn run(self, repo: ConfigRepository<'_>, env_prefix: &str) -> anyhow::Result<()> {
        let mut printer = smart_config_commands::Printer::stdout();
        match self {
            Self::Help(HelpCommandArgs { filter }) => {
                printer.print_help(repo.schema(), |param| {
                    filter.as_ref().is_none_or(|needle| {
                        param.all_paths().any(|(path, _)| path.contains(needle))
                    })
                })?;
            }
            Self::Debug { filter } => {
                printer.print_debug(&repo, |param| {
                    filter.as_ref().is_none_or(|needle| {
                        param.all_paths().any(|(path, _)| path.contains(needle))
                    })
                })??;
            }
            Self::Print { diff, format } => {
                let mut options = if diff {
                    SerializerOptions::diff_with_default()
                } else {
                    SerializerOptions::default()
                };
                options = options.flat(format.is_flat());

                let json = repo.canonicalize(&options)?;
                match format {
                    SerializationFormat::Yaml => {
                        printer.print_yaml(&serde_json::Value::from(json))?;
                    }
                    SerializationFormat::Json => {
                        printer.print_json(&serde_json::Value::from(json))?;
                    }
                    SerializationFormat::EnvYaml => {
                        let mut flat =
                            smart_config::Environment::convert_flat_params(&json, env_prefix);
                        Self::escape_for_docker(&mut flat);
                        printer.print_yaml(&serde_json::Value::from(flat))?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Docker Compose requires nulls and Boolean values to be quoted.
    fn escape_for_docker(map: &mut serde_json::Map<String, serde_json::Value>) {
        for value in map.values_mut() {
            match value {
                serde_json::Value::Null => *value = "null".into(),
                serde_json::Value::Bool(flag) => *value = flag.to_string().into(),
                serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                    unreachable!("arrays / objects should be flattened to strings");
                }
                _ => { /* Leave the value as-is */ }
            }
        }
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
    pub fn run(self, repo: ConfigRepository<'_>, env_prefix: &str) -> anyhow::Result<()> {
        let command = self.command.unwrap_or(ConfigCommand::Help(self.args));
        command.run(repo, env_prefix)
    }
}

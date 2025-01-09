use clap::Parser;

#[derive(Debug, Parser)]
pub struct InsertBatchArgs {
    #[clap(long)]
    pub number: Option<u32>,
    #[clap(long, default_value = "false")]
    pub default: bool,
    #[clap(long)]
    pub version: Option<String>,
}

#[derive(Debug)]
pub struct InsertBatchArgsFinal {
    pub number: u32,
    pub version: String,
}

impl InsertBatchArgs {
    pub(crate) fn fill_values_with_prompts(self, era_version: String) -> InsertBatchArgsFinal {
        let number = self.number.unwrap_or_else(|| {
            zkstack_cli_common::Prompt::new("Enter the number of the batch to insert").ask()
        });

        if self.default {
            return InsertBatchArgsFinal {
                number,
                version: era_version,
            };
        }

        let version = self.version.unwrap_or_else(|| {
            zkstack_cli_common::Prompt::new("Enter the version of the batch to insert")
                .default(&era_version)
                .ask()
        });

        InsertBatchArgsFinal { number, version }
    }
}

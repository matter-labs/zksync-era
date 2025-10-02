use clap::Parser;

#[derive(Debug, Parser)]
pub struct InsertVersionArgs {
    #[clap(long, default_value = "false")]
    pub default: bool,
    #[clap(long)]
    pub version: Option<String>,
    #[clap(long)]
    pub snark_wrapper: Option<String>,
    #[clap(long)]
    pub fflonk_snark_wrapper: Option<String>,
}

#[derive(Debug)]
pub struct InsertVersionArgsFinal {
    pub snark_wrapper: String,
    pub fflonk_snark_wrapper: String,
    pub version: String,
}

impl InsertVersionArgs {
    pub(crate) fn fill_values_with_prompts(
        self,
        era_version: String,
        snark_wrapper: String,
        fflonk_snark_wrapper: String,
    ) -> InsertVersionArgsFinal {
        if self.default {
            return InsertVersionArgsFinal {
                snark_wrapper,
                fflonk_snark_wrapper,
                version: era_version,
            };
        }

        let version = self.version.unwrap_or_else(|| {
            zkstack_cli_common::Prompt::new("Enter the version of the protocol to insert")
                .default(&era_version)
                .ask()
        });

        let snark_wrapper = self.snark_wrapper.unwrap_or_else(|| {
            zkstack_cli_common::Prompt::new("Enter the snark wrapper of the protocol to insert")
                .default(&snark_wrapper)
                .ask()
        });

        let fflonk_snark_wrapper = self.fflonk_snark_wrapper.unwrap_or_else(|| {
            zkstack_cli_common::Prompt::new(
                "Enter the fflonk snark wrapper of the protocol to insert",
            )
            .default(&fflonk_snark_wrapper)
            .ask()
        });

        InsertVersionArgsFinal {
            snark_wrapper,
            fflonk_snark_wrapper,
            version,
        }
    }
}

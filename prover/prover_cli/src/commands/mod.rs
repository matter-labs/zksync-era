use clap::Subcommand;
use status::jobs;

pub(crate) mod get_file_info;
pub(crate) mod status;

#[derive(Subcommand)]
pub(crate) enum StatusCommand {
    Jobs(jobs::Args),
}

use app::App;
use clap::Parser;

pub(crate) mod app;
pub(crate) mod selectors;

/// Selector generator tool.
///
/// Generates a mapping of short (4-byte) function selectors to their corresponding function names.
///
/// The generated JSON can be used to lookup function names by their selectors, when interacting
/// with Ethereum contracts.
#[derive(Debug, Parser)]
#[command(author, version, about, long_about)]
struct Cli {
    /// Path to the directory with JSON files containing ABI.
    /// All JSON files in this directory will be processed.
    contracts_dir: String,
    /// Path to the output file.
    /// The file will contain the list of function selectors.
    /// If the file already exists, new selectors will be appended to it.
    output_file: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    let mut app = App::load(args.output_file).await?;
    app.process_files(&args.contracts_dir).await?;
    app.report();
    app.save().await?;
    Ok(())
}

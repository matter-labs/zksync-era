use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub(crate) struct Args {
    /// File with the basic proof.
    #[clap(short, long)]
    file: String,
}

pub(crate) async fn run(_args: Args) -> anyhow::Result<()> {
    #[cfg(not(feature = "verbose_circuits"))]
    anyhow::bail!("Please compile with verbose_circuits feature");
    #[cfg(feature = "verbose_circuits")]
    {
        let buffer = std::fs::read(_args.file).unwrap();
        zkevm_test_harness::debug::debug_circuit(&buffer);
        Ok(())
    }
}

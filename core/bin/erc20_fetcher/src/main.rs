use clap::Parser;
use erc20_fetcher::{RpcApiBackend, RpcApiServer};
use jsonrpsee::{
    server::{ServerBuilder, ServerHandle},
    RpcModule,
};
use tokio::net::ToSocketAddrs;
use tracing::{error, info};

#[derive(Parser)]
struct Cli {
    #[arg(long, help = "the port where the fetcher will run.")]
    port: u16,
    #[arg(long, help = "the name of the ERC20 token.")]
    name: String,
    #[arg(long, help = "the symbol of the ERC20 token.")]
    symbol: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let opt = Cli::parse();
    let addr = format!("127.0.0.1:{}", opt.port);
    let rpc_module = RpcApiBackend::new(opt.symbol, opt.name).into_rpc();

    let handle = start_server(addr, rpc_module).await;
    match handle {
        Ok(handle) => {
            info!("Server started and listening on port: {}", opt.port);
            handle.stopped().await;
        }
        Err(e) => error!("An error has occurred while starting the server: {}", e),
    }
    Ok(())
}

async fn start_server(
    addr: impl ToSocketAddrs,
    module: RpcModule<RpcApiBackend>,
) -> Result<ServerHandle, anyhow::Error> {
    let server = ServerBuilder::default().build(addr).await?;
    let server_handle = server.start(module);
    Ok(server_handle)
}

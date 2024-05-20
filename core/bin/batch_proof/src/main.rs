use clap::Parser;
use zksync_types::{L1ChainId,L2ChainId,U256,url::SensitiveUrl};
use zksync_web3_decl::client::{Client, L1, L2};
use zksync_web3_decl::namespaces::ZksNamespaceClient as _;
use zksync_eth_client as eth;
use anyhow::Context as _;
use tracing_subscriber::{Registry,prelude::*};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    l1_url: SensitiveUrl,
    #[arg(long)]
    l2_url: SensitiveUrl,
    #[arg(long)]
    l1_chain_id: L1ChainId,
    #[arg(long)]
    l2_chain_id: L2ChainId,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing::subscriber::set_global_default(Registry::default().with(tracing_subscriber::fmt::layer()
        .pretty()
        .with_file(false)
        .with_line_number(false)
        .with_filter(tracing_subscriber::EnvFilter::from_default_env())
    )).unwrap();

    let args = Args::parse();
    let l1_client : Client<L1> = Client::http(args.l1_url)
        .context("Client::http(<L1>)")?
        .for_network(args.l1_chain_id.into())
        .build();
    let l2_client : Client<L2> = Client::http(args.l2_url)
        .context("Client::http(<L2>)")?
        .for_network(args.l2_chain_id.into())
        .build();
    let contract = zksync_contracts::hyperchain_contract();
    let contract_addr = l2_client.get_main_contract().await.context("get_main_contract()")?;
    let version: U256 = eth::CallFunctionArgs::new("getProtocolVersion", ())
        .for_contract(contract_addr, &contract)
        .call(&l1_client)
        .await
        .context("getProtocolVersion()")?;
    tracing::info!("version = {version}");
    Ok(())
}

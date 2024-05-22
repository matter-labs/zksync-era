use clap::Parser;
use zksync_types::{ethabi, web3::keccak256,L1BatchNumber,L1ChainId,L2ChainId,H256,U256,url::SensitiveUrl};
use zksync_web3_decl::client::{Client, L1, L2};
use zksync_web3_decl::namespaces::ZksNamespaceClient as _;
use zksync_dal::{ConnectionPool, Core, CoreDal as _};
use zksync_eth_client as eth;
use anyhow::Context as _;
use tracing_subscriber::{Registry,prelude::*};
use url::Url;
use zksync_l1_contract_interface::{Tokenizable as _, i_executor::structures::StoredBatchInfo};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    l1_url: Option<SensitiveUrl>,
    #[arg(long)]
    l2_url: Option<SensitiveUrl>,
    #[arg(long)]
    l1_chain_id: Option<L1ChainId>,
    #[arg(long)]
    l2_chain_id: Option<L2ChainId>,
    #[arg(long)]
    postgres_url: Option<SensitiveUrl>,
}

impl Args {
    fn l1_url(&self) -> SensitiveUrl {
        self.l1_url.clone().unwrap_or(Url::try_from("https://ethereum-sepolia-rpc.publicnode.com").unwrap().into())
    }

    fn l2_url(&self) -> SensitiveUrl {
        self.l2_url.clone().unwrap_or(Url::try_from("https://z2-dev-api.zksync.dev").unwrap().into())
    }

    fn l1_chain_id(&self) -> L1ChainId {
        self.l1_chain_id.unwrap_or(11155111.into())
    }

    fn l2_chain_id(&self) -> L2ChainId {
        self.l2_chain_id.unwrap_or(270.into())
    }

    fn postgres_url(&self) -> SensitiveUrl {
        self.postgres_url.clone().unwrap_or(Url::try_from(
            "postgres://postgres:hj435kf8jijfk24rjksa9t@142.132.150.73:6782/zksync_local_ext_node"
        ).unwrap().into())
    }
}

pub async fn get_proofs_impl(
    &self,
    address: Address,
    keys: Vec<H256>,
    l1_batch_number: L1BatchNumber,
) -> Result<Option<Proof>, Web3Error> {
    // [0..12] = 0
    // [12..32] = addr
    // [32..64] = key.from_big_endian().to_big_endian()
    // key = Blake2s256([0..64])
    //
    let key = StorageKey::new(AccountTreeId::new(address), key).hashed_key_u256();

    let tree_api = self.state.tree_api.as_deref().unwrap().get_proofs(l1_batch_number, vec![key]).await.unwrap();
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
    let l1_client : Client<L1> = Client::http(args.l1_url())
        .context("Client::http(<L1>)")?
        .for_network(args.l1_chain_id().into())
        .build();
    let l2_client : Client<L2> = Client::http(args.l2_url())
        .context("Client::http(<L2>)")?
        .for_network(args.l2_chain_id().into())
        .build();
    let contract = zksync_contracts::hyperchain_contract();
    let contract_addr = l2_client.get_main_contract().await.context("get_main_contract()")?;
    tracing::info!("contract_addr = {contract_addr}");
    let last_batch : U256 = eth::CallFunctionArgs::new("getTotalBatchesCommitted", ())
        .for_contract(contract_addr, &contract)
        .call(&l1_client)
        .await
        .context("getTotalBatchesCommitted()")?;
    let want_hash : H256 = eth::CallFunctionArgs::new("storedBatchHash", last_batch)
        .for_contract(contract_addr, &contract)
        .call(&l1_client)
        .await
        .context("getTotalBatchesCommitted()")?;

    let pool : ConnectionPool<Core> = ConnectionPool::singleton(args.postgres_url()).build().await.context("ConnectionPool::build()")?;
    let mut conn = pool.connection().await.context("pool.connection()")?;
    let batch = conn.blocks_dal().get_l1_batch_metadata(L1BatchNumber(last_batch.try_into().unwrap())).await?.context("batch not in storage")?;
    let token = StoredBatchInfo(&batch).into_token();
    let got_hash = H256(keccak256(&ethabi::encode(&[token])));

    tracing::info!("batch[{last_batch}] = got {got_hash}, want {want_hash}");
    tracing::info!("store_root_hash = {}",batch.metadata.root_hash);
    // contracts/system-contracts/contracts-preprocessed/SystemContext.sol
    // we need deployment address and the offset of the interesting field.
    // https://forum.soliditylang.org/t/storage-object-json-interface/378/8
    //
    // ~/Downloads/solc-linux-amd64-v0.8.20+commit.a1b79de6 SystemContext.sol --storage-layout
    // {"astId":71,"contract":"SystemContext.sol:SystemContext","label":"l2BlockHash","offset":0,"slot":"11","type":"t_array(t_bytes32)257_storage"}
    // slot[block_number] = 11 + block_number%257
    


    Ok(())
}

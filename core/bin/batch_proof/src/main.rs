use clap::Parser;
use zksync_system_constants as constants;
use zksync_types::{block::unpack_block_info, L2BlockNumber, block::L2BlockHasher, ethabi, Address, web3::keccak256,commitment::L1BatchWithMetadata, L1BatchNumber,L1ChainId,L2ChainId,H256,U256,url::SensitiveUrl};
use zksync_web3_decl::client as web3;
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
    postgres_url: SensitiveUrl,
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
}

/*pub async fn get_proofs_impl(
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
}*/

struct Client {
    l1_contract: ethabi::Contract,
    l1_contract_addr: Address,
    l1: web3::Client<web3::L1>,
    l2: web3::Client<web3::L2>,
    pool: ConnectionPool<Core>,
}

impl Client {
    async fn new(args: &Args) -> anyhow::Result<Self> {
        let l1 : web3::Client<web3::L1> = web3::Client::http(args.l1_url())
            .context("Client::http(<L1>)")?
            .for_network(args.l1_chain_id().into())
            .build();
        let l2 : web3::Client<web3::L2> = web3::Client::http(args.l2_url())
            .context("Client::http(<L2>)")?
            .for_network(args.l2_chain_id().into())
            .build();
        let l1_contract = zksync_contracts::hyperchain_contract();
        Ok(Self {
            l1_contract_addr: l2.get_main_contract().await.context("get_main_contract()")?,
            l1, l2, l1_contract,
            pool: ConnectionPool::singleton(args.postgres_url.clone()).build().await.context("ConnectionPool::build()")?,
        })
    }

    async fn l1_last_batch(&self) -> anyhow::Result<L1BatchNumber> {
        let last_batch : U256 = eth::CallFunctionArgs::new("getTotalBatchesCommitted", ())
            .for_contract(self.l1_contract_addr, &self.l1_contract)
            .call(&self.l1)
            .await
            .context("getTotalBatchesCommitted()")?;
        Ok(L1BatchNumber(last_batch.try_into().map_err(|err|anyhow::format_err!("L1BatchNumber overflow: {err}"))?))
    }

    async fn l1_batch_hash(&self, n: L1BatchNumber) -> anyhow::Result<H256> {
        eth::CallFunctionArgs::new("storedBatchHash", U256::from(n.0))
            .for_contract(self.l1_contract_addr, &self.l1_contract)
            .call(&self.l1)
            .await
            .context("getTotalBatchesCommitted()")
    }

    async fn db_batch(&self, n: L1BatchNumber) -> anyhow::Result<L1BatchWithMetadata> {
        let mut conn = self.pool.connection().await.context("pool.connection()")?;
        conn.blocks_dal().get_l1_batch_metadata(n).await?.context("batch not in storage")
    }

    async fn l2_block_hash(&self, n: L1BatchNumber) -> anyhow::Result<(L2BlockNumber,H256)> {
        let addr = constants::SYSTEM_CONTEXT_ADDRESS;
        let key = constants::SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION; 
        let block_info = self.l2.get_proof(addr,vec![key],n).await.context("get_proof()")?.context("missing proof")?;
        let block_info = &block_info.storage_proof[0];
        let (block_number,_) = unpack_block_info(block_info.value.as_bytes().into());
        tracing::info!("block_number = {block_number}");

        let key = U256::from(constants::SYSTEM_CONTEXT_CURRENT_L2_BLOCK_HASHES_POSITION.as_bytes()) + 
            U256::from(block_number)%U256::from(constants::SYSTEM_CONTEXT_STORED_L2_BLOCK_HASHES);
        let key = H256::from(<[u8;32]>::from(key));
        let block_hash = self.l2.get_proof(addr,vec![key],n).await.context("get_proof()")?.context("missing proof")?;
        Ok((L2BlockNumber(block_number.try_into().unwrap()),block_hash.storage_proof[0].value))
        // TODO: this should generate a proof.
    }
}

fn batch_hash(batch: &L1BatchWithMetadata) -> H256 {
    let token = StoredBatchInfo(batch).into_token();
    H256(keccak256(&ethabi::encode(&[token])))
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
    let c = Client::new(&args).await.context("Client::new()")?;
    let last_batch = c.l1_last_batch().await.context("l1_last_batch()")?;
    let l1_batch_hash = c.l1_batch_hash(last_batch).await.context("l1_batch_hash()")?;
    let db_batch = c.db_batch(last_batch).await.context("db_batch()")?;
    let db_batch_hash = batch_hash(&db_batch);
    tracing::info!("batch[{last_batch}] = db {db_batch_hash}, l1 {l1_batch_hash}");
    tracing::info!("store_root_hash = {}",db_batch.metadata.root_hash);
    // contracts/system-contracts/contracts-preprocessed/SystemContext.sol
    // we need deployment address and the offset of the interesting field.
    // https://forum.soliditylang.org/t/storage-object-json-interface/378/8
    //
    // ~/Downloads/solc-linux-amd64-v0.8.20+commit.a1b79de6 SystemContext.sol --storage-layout
    // {"astId":71,"contract":"SystemContext.sol:SystemContext","label":"l2BlockHash","offset":0,"slot":"11","type":"t_array(t_bytes32)257_storage"}
    // {"astId":63,"contract":"SystemContext.sol:SystemContext","label":"currentL2BlockInfo","offset":0,"slot":"9","type":"t_struct(BlockInfo)1434_storage"}
    // slot[block_number] = 11 + block_number%257
   
    let (last,last_hash) = c.l2_block_hash(last_batch).await.context("l2_block_hash(last)")?;
    let (prev,mut prev_hash) = c.l2_block_hash(last_batch-1).await.context("l2_block_hash(prev)")?;
    tracing::info!("last = {last_hash}, prev = {prev_hash}");

    let mut conn = c.pool.connection().await?;
    let block = conn.sync_dal().sync_block(prev,true).await?.context("sync_block()")?;
    assert_eq!(block.hash.unwrap(),prev_hash);

    for i in (prev.0+1..=last.0).map(L2BlockNumber) {
        let block = conn.sync_dal().sync_block(i,true).await?.context("sync_block()")?;
        let mut hasher = L2BlockHasher::new(  
            block.number,
            block.timestamp,
            prev_hash,
        );
        for tx in block.transactions.unwrap() {
            hasher.push_tx_hash(tx.hash());
        }
        // TODO: protocol version should be consistent across blocks and same
        // as protocol version of the L1 batch. Also check it against L1.
        prev_hash = hasher.finalize(block.protocol_version);
        tracing::info!("hash(block({i})) = {prev_hash}");
    }
    assert_eq!(prev_hash,last_hash);
    Ok(())
}

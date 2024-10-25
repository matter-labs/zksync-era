use std::{cmp, fs::File, future::Future, num::NonZero, str::FromStr, sync::Arc};

use anyhow::{anyhow, Result};
use ethabi::{Contract, Event, Function};
use rand::random;
use thiserror::Error;
use tokio::{
    sync::Mutex,
    time::{sleep, Duration},
};
use tracing::log::__private_api::log;
use zksync_basic_types::{
    url::SensitiveUrl,
    web3::{BlockId, BlockNumber, FilterBuilder, Transaction},
    Address, H256, U64,
};
use zksync_eth_client::EthInterface;
use zksync_utils::env::Workspace;
use zksync_web3_decl::client::{Client, DynClient, L1};

use crate::{
    l1_fetcher::{
        blob_http_client::BlobHttpClient,
        constants::ethereum::{BLOB_BLOCK, BOOJUM_BLOCK, ZK_SYNC_ADDR},
        types::{v1::V1, v2::V2, CommitBlock, ParseError},
    },
    storage::reconstruction::ReconstructionDatabase,
};

/// `MAX_RETRIES` is the maximum number of retries on failed L1 call.
const MAX_RETRIES: u8 = 5;
/// The interval in seconds to wait before retrying to fetch a previously failed transaction.
const FAILED_FETCH_RETRY_INTERVAL_S: u64 = 10;
/// The interval in seconds in which to print metrics.
const METRICS_PRINT_INTERVAL_S: u64 = 10;

#[allow(clippy::enum_variant_names)]
#[derive(Error, Debug)]
pub enum L1FetchError {
    #[error("get logs failed")]
    GetLogs,

    #[error("get tx failed")]
    GetTx,

    #[error("get end block number failed")]
    GetEndBlockNumber,
}

#[derive(Debug, Clone)]
pub struct L1FetcherConfig {
    /// The Ethereum JSON-RPC HTTP URL to use.
    pub http_url: String,
    /// The Ethereum blob storage URL base.
    pub blobs_url: String,
    /// The amount of blocks to step over on each log iterration.
    pub block_step: u64,
}

#[derive(Debug, Clone)]
struct Contracts {
    v1: Contract,
    v2: Contract,
}

#[derive(Debug)]
pub struct L1Fetcher {
    eth_client: Box<DynClient<L1>>,
    contracts: Contracts,
    config: L1FetcherConfig,
    inner_db: Option<Arc<Mutex<ReconstructionDatabase>>>,
}

impl L1Fetcher {
    pub fn new(
        config: L1FetcherConfig,
        inner_db: Option<Arc<Mutex<ReconstructionDatabase>>>,
    ) -> Result<Self> {
        let url = SensitiveUrl::from_str(&config.http_url).unwrap();
        let eth_client = Client::http(url)
            .unwrap()
            .with_allowed_requests_per_second(NonZero::new(10).unwrap())
            .build();

        let eth_client = Box::new(eth_client) as Box<DynClient<L1>>;

        let v1 = Self::v1_contract()?;
        let v2 = Self::v2_contract()?;
        let contracts = Contracts { v1, v2 };

        Ok(L1Fetcher {
            eth_client,
            contracts,
            config,
            inner_db,
        })
    }

    fn v1_contract() -> Result<Contract> {
        let base_path = Workspace::locate().core();
        let path = base_path.join("core/node/l1_recovery/abi/IZkSync.json");
        tracing::info!("Loading contract from {path:?}");
        Ok(Contract::load(File::open(path)?)?)
    }

    fn v2_contract() -> Result<Contract> {
        let base_path = Workspace::locate().core();
        let path = base_path.join("core/node/l1_recovery/abi/IZkSyncV2.json");
        tracing::info!("Loading contract from {path:?}");
        Ok(Contract::load(File::open(path)?)?)
    }
    fn commit_functions() -> Result<Vec<Function>> {
        let v2_contract = L1Fetcher::v2_contract()?;
        Ok(vec![
            v2_contract.functions_by_name("commitBatches").unwrap()[0].clone(),
            v2_contract
                .functions_by_name("commitBatchesSharedBridge")
                .unwrap()[0]
                .clone(),
        ])
    }

    fn block_commit_event() -> Result<Event> {
        Ok(L1Fetcher::v1_contract()?.events_by_name("BlockCommit")?[0].clone())
    }

    pub async fn get_all_blocks_to_process(&self) -> Vec<CommitBlock> {
        let start_block = self.get_first_commit_batch_block_number().await;
        let end_block = L1Fetcher::get_last_l1_block_number(&self.eth_client)
            .await
            .unwrap();

        self.get_blocks_to_process(start_block, end_block).await
    }

    pub async fn get_first_commit_batch_block_number(&self) -> U64 {
        let mut current = U64::zero();
        let mut step = U64::from(1 << 30);
        let end_block = L1Fetcher::get_last_l1_block_number(&self.eth_client)
            .await
            .unwrap();

        let event = L1Fetcher::block_commit_event().unwrap();

        while step != U64::zero() {
            while current + step >= end_block {
                step /= 2;
                continue;
            }

            let filter_from_block = current + step;
            // 7 days of blocks, there should be at least one commit batch there
            let filter_to_block = cmp::min(filter_from_block + 50000, end_block);
            let filter = FilterBuilder::default()
                .address(vec![ZK_SYNC_ADDR.parse::<Address>().unwrap()])
                .topics(Some(vec![event.signature()]), None, None, None)
                .from_block(BlockNumber::Number(filter_to_block))
                .to_block(BlockNumber::Number(filter_to_block))
                .build();

            // Grab all relevant logs.
            let logs = L1Fetcher::retry_call(
                || L1Fetcher::query_client(&self.eth_client).logs(&filter),
                L1FetchError::GetLogs,
            )
            .await
            .unwrap();
            if logs.len() == 0 {}
        }

        U64::zero()
    }

    pub async fn get_blocks_to_process(
        &self,
        start_block: U64,
        end_block: U64,
    ) -> Vec<CommitBlock> {
        assert!(start_block <= end_block);

        let event = L1Fetcher::block_commit_event().unwrap();
        let client = BlobHttpClient::new(self.config.blobs_url.clone()).unwrap();
        let functions = L1Fetcher::commit_functions().unwrap();
        let block_step = self.config.block_step;

        let mut current_block = start_block;
        let mut result = vec![];

        loop {
            let filter_to_block = cmp::min(current_block + block_step - 1, end_block);
            let filter = FilterBuilder::default()
                .address(vec![ZK_SYNC_ADDR.parse::<Address>().unwrap()])
                .topics(Some(vec![event.signature()]), None, None, None)
                .from_block(BlockNumber::Number(current_block))
                .to_block(BlockNumber::Number(filter_to_block))
                .build();

            // Grab all relevant logs.
            let logs = L1Fetcher::retry_call(
                || L1Fetcher::query_client(&self.eth_client).logs(&filter),
                L1FetchError::GetLogs,
            )
            .await
            .unwrap();
            tracing::info!(
                "Found {} logs for blocks: {current_block}-{filter_to_block}",
                logs.len()
            );

            for log in logs {
                let hash = log.transaction_hash.unwrap();
                let tx = L1Fetcher::retry_call(
                    || L1Fetcher::get_transaction_by_hash(&self.eth_client, hash),
                    L1FetchError::GetTx,
                )
                .await
                .unwrap();
                let mut blocks = L1Fetcher::process_tx_data(&functions, &client, tx)
                    .await
                    .unwrap();
                result.append(&mut blocks);
            }

            if filter_to_block == end_block {
                tracing::info!("Fetching finished...");
                break;
            }
            current_block = filter_to_block + 1;
        }
        return result;
    }

    async fn process_tx_data(
        commit_functions: &[Function],
        blob_client: &BlobHttpClient,
        tx: Transaction,
    ) -> Result<Vec<CommitBlock>, ParseError> {
        tracing::info!(
            "Processing commit transactions from ethereum block: {:?}, tx_hash: {:?}",
            tx.block_number,
            tx.hash
        );

        let block_number = tx.block_number.unwrap().as_u64();
        loop {
            match parse_calldata(block_number, &commit_functions, &tx.input.0, &blob_client).await {
                Ok(blks) => break Ok(blks),
                Err(e) => {
                    match e.clone() {
                        ParseError::BlobStorageError(err) => {
                            tracing::error!("Blob storage error {err}");
                        }
                        ParseError::BlobFormatError(data, inner) => {
                            tracing::error!("Cannot parse {}: {}", data, inner);
                        }
                        _ => {
                            tracing::error!("Failed to parse calldata: {e}, encountered on block: {block_number}");
                        }
                    }
                    return Err(e);
                }
            }
        }
    }

    /// Get a specified transaction on L1 by its hash.
    async fn get_transaction_by_hash(
        eth_client: &Box<DynClient<L1>>,
        hash: H256,
    ) -> Result<Transaction> {
        match L1Fetcher::retry_call(
            || L1Fetcher::query_client(eth_client).get_tx(hash),
            L1FetchError::GetTx,
        )
        .await
        {
            Ok(Some(tx)) => Ok(tx),
            Ok(None) => Err(anyhow!("unable to find transaction with hash: {}", hash)),
            Err(e) => Err(e),
        }
    }

    fn query_client(eth_client: &Box<DynClient<L1>>) -> &dyn EthInterface {
        eth_client
    }
    /// Get the last published L1 block.
    async fn get_last_l1_block_number(eth_client: &Box<DynClient<L1>>) -> Result<U64> {
        let last_block = L1Fetcher::retry_call(
            || L1Fetcher::query_client(eth_client).block(BlockId::Number(BlockNumber::Finalized)),
            L1FetchError::GetEndBlockNumber,
        )
        .await?;

        last_block
            .expect("last block must be present")
            .number
            .ok_or_else(|| anyhow!("found latest block, but it contained no block number"))
    }

    async fn retry_call<T, E, Fut>(callback: impl Fn() -> Fut, err: L1FetchError) -> Result<T>
    where
        Fut: Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        for attempt in 1..=MAX_RETRIES {
            match callback().await {
                Ok(x) => return Ok(x),
                Err(e) => {
                    tracing::error!("attempt {attempt}: failed to fetch from L1: {e}");
                    sleep(Duration::from_millis(2000 + random::<u64>() % 500)).await;
                }
            }
        }
        Err(err.into())
    }
}
pub async fn parse_calldata(
    l1_block_number: u64,
    commit_candidates: &[Function],
    calldata: &[u8],
    client: &BlobHttpClient,
) -> Result<Vec<CommitBlock>, ParseError> {
    if calldata.len() < 4 {
        return Err(ParseError::InvalidCalldata("too short".to_string()));
    }

    let commit_fn = commit_candidates
        .iter()
        .find(|f| f.short_signature() == calldata[..4])
        .ok_or_else(|| ParseError::InvalidCalldata("signature not found".to_string()))?;
    let mut parsed_input = commit_fn
        .decode_input(&calldata[4..])
        .map_err(|e| ParseError::InvalidCalldata(e.to_string()))?;

    let argc = parsed_input.len();
    if argc != 2 && argc != 3 {
        return Err(ParseError::InvalidCalldata(format!(
            "invalid number of parameters (got {}, expected 2 or 3) for commitBlocks function",
            argc
        )));
    }

    let new_blocks_data = parsed_input
        .pop()
        .ok_or_else(|| ParseError::InvalidCalldata("new blocks data".to_string()))?;
    let stored_block_info = parsed_input
        .pop()
        .ok_or_else(|| ParseError::InvalidCalldata("stored block info".to_string()))?;

    let ethabi::Token::Tuple(stored_block_info) = stored_block_info else {
        return Err(ParseError::InvalidCalldata(
            "invalid StoredBlockInfo".to_string(),
        ));
    };

    let ethabi::Token::Uint(_previous_l1_batch_number) = stored_block_info[0].clone() else {
        return Err(ParseError::InvalidStoredBlockInfo(
            "cannot parse previous L1 batch number".to_string(),
        ));
    };

    let ethabi::Token::Uint(_previous_enumeration_index) = stored_block_info[2].clone() else {
        return Err(ParseError::InvalidStoredBlockInfo(
            "cannot parse previous enumeration index".to_string(),
        ));
    };

    // Parse blocks using [`CommitBlockInfoV1`] or [`CommitBlockInfoV2`]
    let block_infos = parse_commit_block_info(&new_blocks_data, l1_block_number, client).await?;
    Ok(block_infos)
}

async fn parse_commit_block_info(
    data: &ethabi::Token,
    l1_block_number: u64,
    client: &BlobHttpClient,
) -> Result<Vec<CommitBlock>, ParseError> {
    let ethabi::Token::Array(data) = data else {
        return Err(ParseError::InvalidCommitBlockInfo(
            "cannot convert newBlocksData to array".to_string(),
        ));
    };

    let mut result = vec![];
    for d in data {
        let commit_block = {
            if l1_block_number >= BLOB_BLOCK {
                CommitBlock::try_from_token_resolve(d, client).await?
            } else if l1_block_number >= BOOJUM_BLOCK {
                CommitBlock::try_from_token::<V2>(d)?
            } else {
                CommitBlock::try_from_token::<V1>(d)?
            }
        };

        result.push(commit_block);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap, env::temp_dir, num::NonZero, os::linux::raw::stat, str::FromStr,
    };

    use tempfile::TempDir;
    use zksync_basic_types::{url::SensitiveUrl, L1BatchNumber, L2BlockNumber, H256, U64};
    use zksync_dal::{ConnectionPool, Core, CoreDal};
    use zksync_types::snapshots::SnapshotRecoveryStatus;
    use zksync_utils::{bytecode::hash_bytecode, env::Workspace};
    use zksync_web3_decl::client::{Client, DynClient, L1};

    use crate::{
        l1_fetcher::{
            blob_http_client::BlobHttpClient,
            constants::sepolia_initial_state_path,
            l1_fetcher::{L1Fetcher, L1FetcherConfig},
        },
        processor::{snapshot::StateCompressor, tree::TreeProcessor, Processor},
    };

    fn sepolia_client() -> Box<DynClient<L1>> {
        let url = SensitiveUrl::from_str(&"https://ethereum-sepolia-rpc.publicnode.com").unwrap();
        let eth_client = Client::http(url)
            .unwrap()
            .with_allowed_requests_per_second(NonZero::new(10).unwrap())
            .build();
        Box::new(eth_client)
    }

    fn sepolia_l1_fetcher() -> L1Fetcher {
        let config = L1FetcherConfig {
            http_url: "https://ethereum-sepolia-rpc.publicnode.com".to_string(),
            blobs_url: "https://api.sepolia.blobscan.com/blobs/".to_string(),
            block_step: 1000,
        };
        L1Fetcher::new(config, None).unwrap()
    }
    #[test_log::test(tokio::test)]
    async fn uncompressing_factory_deps_from_l2_to_l1_messages() {
        let eth_client = sepolia_client();
        // commitBatch no. 403 from boojnet
        let tx = L1Fetcher::get_transaction_by_hash(
            &eth_client,
            H256::from_str("0x0624f45cf7ad6c3fe12c8c1d320ecdbd051ce0a6b79888c5754ee6f5b31d9ae6")
                .unwrap(),
        )
        .await
        .unwrap();
        let blob_provider =
            BlobHttpClient::new("https://api.sepolia.blobscan.com/blobs/".to_string()).unwrap();
        let functions = L1Fetcher::commit_functions().unwrap();
        let blocks = L1Fetcher::process_tx_data(&functions, &blob_provider, tx)
            .await
            .unwrap();
        assert_eq!(1, blocks.len());
        let block = &blocks[0];
        assert_eq!(9, block.factory_deps.len());
        assert_eq!(
            hash_bytecode(&block.factory_deps[0]),
            H256::from_str("0x010000418c2c6cddb87cc900cb58ff9fd387862d6f77d4e7d40dc35694ba1ae4")
                .unwrap()
        );
    }

    #[test_log::test(tokio::test)]
    async fn uncompressing_factory_deps_from_l2_to_l1_messages_for_blob_batch() {
        tracing::info!("{:?}", Workspace::locate().core());

        let eth_client = sepolia_client();
        // commitBatch no. 11944 from boojnet, pubdata submitted using blobs
        let tx = L1Fetcher::get_transaction_by_hash(
            &eth_client,
            H256::from_str("0xd3778819cfa599e06b7d24fceff81de273c16a2cd2a1556e0ab268545acbe3ae")
                .unwrap(),
        )
        .await
        .unwrap();
        let blob_provider =
            BlobHttpClient::new("https://api.sepolia.blobscan.com/blobs/".to_string()).unwrap();
        let functions = L1Fetcher::commit_functions().unwrap();
        let blocks = L1Fetcher::process_tx_data(&functions, &blob_provider, tx)
            .await
            .unwrap();
        assert_eq!(1, blocks.len());
        let block = &blocks[0];
        assert_eq!(6, block.factory_deps.len());
        assert_eq!(
            hash_bytecode(&block.factory_deps[0]),
            H256::from_str("0x0100009ffa898e7aa96817a2267079a0c8e92ce719b16b2be4cb17a4db04b0e2")
                .unwrap()
        );
    }

    #[test_log::test(tokio::test)]
    async fn get_blocks_to_process_works_correctly() {
        let l1_fetcher = sepolia_l1_fetcher();
        // batches from 10 to 109
        let blocks = l1_fetcher
            .get_blocks_to_process(U64::from(4801161), U64::from(4820508))
            .await;

        // blocks from 10 to 109, two blocks were sent twice (precisely ...80,81,82,83,82,83,84...)
        // we don't deduplicate at this step
        assert_eq!(blocks.len(), 102);
    }

    #[test_log::test(tokio::test)]
    async fn test_process_blocks() {
        let temp_dir = TempDir::new().unwrap().into_path().join("db");

        let l1_fetcher = sepolia_l1_fetcher();
        // batches up to batch 109, we want that many to test whether "duplicated" batches don't break anything
        let blocks = l1_fetcher
            .get_blocks_to_process(U64::from(4800000), U64::from(4820508))
            .await;

        let mut processor = StateCompressor::new(temp_dir).await;
        processor
            .process_genesis_state(sepolia_initial_state_path())
            .await;
        processor.process_blocks(blocks).await;

        assert_eq!(2, processor.export_factory_deps().await.len());

        let temp_dir2 = TempDir::new().unwrap().into_path().join("db2");
        let mut reconstructed = TreeProcessor::new(temp_dir2).await.unwrap();
        reconstructed
            .process_snapshot_storage_logs(processor.export_storage_logs().await)
            .await
            .unwrap();
        // taken from https://sepolia.explorer.zksync.io/batch/109
        assert_eq!(
            "0x575d0a455a544bf171783c5be9db312b6af771ad6844f4eef89d7f2e642bfb90",
            format!("{:?}", reconstructed.get_root_hash())
        );

        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut storage = connection_pool
            .connection_tagged("l1_recovery")
            .await
            .unwrap();

        let dummy_miniblock_number = L2BlockNumber(1);
        storage
            .storage_logs_dal()
            .insert_storage_logs_from_snapshot(
                dummy_miniblock_number,
                &processor.export_storage_logs().await,
            )
            .await
            .unwrap();

        let chunk_deps_hashmap: HashMap<H256, Vec<u8>> = processor
            .export_factory_deps()
            .await
            .iter()
            .map(|dep| (hash_bytecode(&dep.bytecode.0), dep.bytecode.0.clone()))
            .collect();
        storage
            .factory_deps_dal()
            .insert_factory_deps(dummy_miniblock_number, &chunk_deps_hashmap)
            .await
            .unwrap();

        let status = SnapshotRecoveryStatus {
            l1_batch_number: L1BatchNumber(109),
            l1_batch_root_hash: reconstructed.get_root_hash(),
            l1_batch_timestamp: 0,
            l2_block_number: dummy_miniblock_number,
            l2_block_hash: H256::zero(),
            l2_block_timestamp: 0,
            protocol_version: Default::default(),
            storage_logs_chunks_processed: vec![true],
        };
        storage
            .snapshot_recovery_dal()
            .insert_initial_recovery_status(&status)
            .await
            .unwrap()
    }
}

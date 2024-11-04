use std::{cmp, collections::HashMap, fs::File, future::Future, sync::Arc};

use anyhow::{anyhow, Result};
use ethabi::{Contract, Event, Function, Token};
use rand::random;
use thiserror::Error;
use tokio::time::{sleep, Duration};
use zksync_basic_types::{
    web3::{BlockId, BlockNumber, CallRequest, FilterBuilder, Log, Transaction},
    Address, L1BatchNumber, L1BlockNumber, H256, U256, U64,
};
use zksync_contracts::eth_contract;
use zksync_eth_client::{CallFunctionArgs, EthInterface};
use zksync_types::{l1::L1Tx, L2_BASE_TOKEN_ADDRESS};
use zksync_utils::{address_to_u256, bytecode::hash_bytecode, env::Workspace};
use zksync_web3_decl::client::{DynClient, L1};

use crate::l1_fetcher::{
    blob_http_client::BlobClient,
    types::{v1::V1, v2::V2, CommitBlock, ParseError},
};

/// `MAX_RETRIES` is the maximum number of retries on failed L1 call.
const MAX_RETRIES: u8 = 5;

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
pub enum ProtocolVersioning {
    OnlyV3,
    AllVersions {
        v2_start_batch_number: u64,
        v3_start_batch_number: u64,
    },
}

#[derive(Debug, Clone)]
pub struct L1FetcherConfig {
    pub block_step: u64,

    pub diamond_proxy_addr: Address,

    pub versioning: ProtocolVersioning,
}

pub struct L1Fetcher {
    eth_client: Box<DynClient<L1>>,
    blob_client: Arc<dyn BlobClient>,
    config: L1FetcherConfig,
}

impl L1Fetcher {
    pub fn new(
        config: L1FetcherConfig,
        eth_client: Box<DynClient<L1>>,
        blob_client: Arc<dyn BlobClient>,
    ) -> Result<Self> {
        Ok(L1Fetcher {
            eth_client,
            config,
            blob_client,
        })
    }

    fn v1_contract() -> Result<Contract> {
        let base_path = Workspace::locate().core();
        let path = base_path.join("core/node/l1_recovery/abi/IZkSync.json");
        Ok(Contract::load(File::open(path)?)?)
    }

    fn v2_contract() -> Result<Contract> {
        let base_path = Workspace::locate().core();
        let path = base_path.join("core/node/l1_recovery/abi/IZkSyncV2.json");
        Ok(Contract::load(File::open(path)?)?)
    }
    fn commit_functions() -> Result<Vec<Function>> {
        let contract = L1Fetcher::v2_contract()?;
        Ok(vec![
            contract.functions_by_name("commitBatches").unwrap()[0].clone(),
            contract
                .functions_by_name("commitBatchesSharedBridge")
                .unwrap()[0]
                .clone(),
        ])
    }

    fn get_first_unprocessed_priority_tx_function() -> Result<Function> {
        let contract = L1Fetcher::v1_contract()?;
        Ok(contract
            .functions_by_name("getFirstUnprocessedPriorityTx")
            .unwrap()[0]
            .clone())
    }

    fn block_commit_event() -> Result<Event> {
        Ok(L1Fetcher::v1_contract()?.events_by_name("BlockCommit")?[0].clone())
    }

    fn priority_tx_event() -> Result<Event> {
        Ok(L1Fetcher::v1_contract()?.events_by_name("NewPriorityRequest")?[0].clone())
    }

    pub async fn get_all_blocks_to_process(&self) -> Vec<CommitBlock> {
        let start_block = U64::zero();
        let end_block = L1Fetcher::get_last_l1_block_number(&self.eth_client)
            .await
            .unwrap();
        self.get_blocks_to_process(start_block, end_block).await
    }

    pub async fn get_commit_event_eth_block(&self, l1_batch_number: L1BatchNumber) -> Option<u64> {
        let end_block = L1Fetcher::get_last_l1_block_number(&self.eth_client)
            .await
            .unwrap();
        let mut current_block = end_block;
        let event = Self::block_commit_event().unwrap();
        loop {
            let filter_from_block = current_block
                .checked_sub(U64::from(self.config.block_step - 1))
                .unwrap_or_default();
            tracing::info!("checking {} - {}", filter_from_block, current_block);
            let logs = self
                .get_logs(filter_from_block, current_block, &event)
                .await;
            for log in &logs {
                if log.topics[1].to_low_u64_be() as u32 == l1_batch_number.0 {
                    return Some(log.block_number.unwrap().as_u64());
                }
            }
            if filter_from_block == U64::zero() {
                return None;
            }
            current_block = filter_from_block - 1;
        }
    }

    async fn get_logs(&self, start_block: U64, end_block: U64, event: &Event) -> Vec<Log> {
        assert!(end_block - start_block <= U64::from(self.config.block_step));
        let filter = FilterBuilder::default()
            .address(vec![self.config.diamond_proxy_addr])
            .topics(Some(vec![event.signature()]), None, None, None)
            .from_block(BlockNumber::Number(start_block))
            .to_block(BlockNumber::Number(end_block))
            .build();

        L1Fetcher::retry_call(
            || L1Fetcher::query_client(&self.eth_client).logs(&filter),
            L1FetchError::GetLogs,
        )
        .await
        .unwrap()
    }
    pub async fn calculate_last_processed_priority_transaction_eth_block(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> L1BlockNumber {
        // TODO we shouldn't just take next block as there may be multiple commit transactions in the same block
        let commit_eth_block = self
            .get_commit_event_eth_block(l1_batch_number)
            .await
            .unwrap()
            + 1;
        tracing::info!(
            "Found commit event for block {l1_batch_number} on eth block {commit_eth_block}"
        );

        let first_unprocessed_priority_id: U256 =
            CallFunctionArgs::new("getFirstUnprocessedPriorityTx", ())
                .with_block(BlockId::Number(BlockNumber::Number(U64::from(
                    commit_eth_block,
                ))))
                .for_contract(
                    self.config.diamond_proxy_addr,
                    &Self::v1_contract().unwrap(),
                )
                .call(&self.eth_client)
                .await
                .unwrap();
        let first_unprocessed_priority_id = first_unprocessed_priority_id.as_u64();

        tracing::info!(
            "First unprocessed priority tx id: {:?}",
            first_unprocessed_priority_id
        );

        let end_block = U64::from(commit_eth_block);
        let mut current_block = end_block;
        //We're looking for last processed priority transaction, it's okay to do a simple
        // linear search here as the tx must be in 50k preceding commit event anyway
        loop {
            let filter_from_block = current_block
                .checked_sub(U64::from(self.config.block_step - 1))
                .unwrap_or_default();

            for tx in self.fetch_priority_txs(filter_from_block, end_block).await {
                if tx.serial_id().0 == first_unprocessed_priority_id - 1 {
                    return tx.eth_block();
                }
            }

            if filter_from_block == U64::zero() {
                panic!(
                    "Unable to find priority tx with id {}",
                    first_unprocessed_priority_id - 1
                )
            }
            current_block = filter_from_block - 1;
        }
    }

    async fn fetch_priority_txs(&self, start_block: U64, end_block: U64) -> Vec<L1Tx> {
        let event = L1Fetcher::priority_tx_event().unwrap();
        let logs = self.get_logs(start_block, end_block, &event).await;
        logs.iter()
            .map(|log| L1Tx::try_from(log.clone()).unwrap())
            .collect()
    }

    pub async fn get_blocks_to_process(
        &self,
        start_block: U64,
        end_block: U64,
    ) -> Vec<CommitBlock> {
        assert!(start_block <= end_block);

        let event = L1Fetcher::block_commit_event().unwrap();
        let functions = L1Fetcher::commit_functions().unwrap();
        let block_step = self.config.block_step;

        let mut current_block = start_block;
        let mut result = vec![];
        let mut priority_txs = vec![];
        let mut priority_txs_so_far = 0;
        let mut last_processed_priority_tx = 0;
        let mut factory_deps_hashes = HashMap::new();

        loop {
            let filter_to_block = cmp::min(current_block + block_step - 1, end_block);
            priority_txs.extend(
                self.fetch_priority_txs(current_block, filter_to_block)
                    .await,
            );

            // Grab all relevant logs.
            let logs = self.get_logs(current_block, filter_to_block, &event).await;
            tracing::info!(
                "Found {} committed blocks and {} priority txs for blocks: {current_block}-{filter_to_block}",
                logs.len(), priority_txs.len() - priority_txs_so_far
            );
            priority_txs_so_far = priority_txs.len();

            for log in logs {
                let hash = log.transaction_hash.unwrap();
                let tx = L1Fetcher::retry_call(
                    || L1Fetcher::get_transaction_by_hash(&self.eth_client, hash),
                    L1FetchError::GetTx,
                )
                .await
                .unwrap();
                let blocks = L1Fetcher::process_tx_data(
                    &functions,
                    &self.blob_client,
                    tx,
                    &self.config.versioning,
                )
                .await
                .unwrap();

                for mut block in blocks {
                    for _ in 0..block.priority_operations_count {
                        let priority_tx = priority_txs[last_processed_priority_tx].clone();
                        tracing::info!(
                            "Processing priority tx: {} with {} factory deps",
                            priority_tx.serial_id(),
                            priority_tx.execute.factory_deps.len()
                        );
                        for factory_dep in &priority_tx.execute.factory_deps {
                            let hashed = hash_bytecode(factory_dep);
                            if factory_deps_hashes.contains_key(&hashed) {
                                continue;
                            } else {
                                factory_deps_hashes.insert(hashed, ());
                                block.factory_deps.push(factory_dep.clone());
                            }
                        }
                        last_processed_priority_tx += 1;
                    }
                    result.push(block)
                }
            }

            if filter_to_block == end_block {
                break;
            }
            current_block = filter_to_block + 1;
        }
        return result;
    }

    async fn process_tx_data(
        commit_functions: &[Function],
        blob_client: &Arc<dyn BlobClient>,
        tx: Transaction,
        protocol_versioning: &ProtocolVersioning,
    ) -> Result<Vec<CommitBlock>, ParseError> {
        let block_number = tx.block_number.unwrap().as_u64();
        loop {
            match parse_calldata(
                protocol_versioning,
                block_number,
                &commit_functions,
                &tx.input.0,
                &blob_client,
            )
            .await
            {
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
    protocol_versioning: &ProtocolVersioning,
    l1_block_number: u64,
    commit_candidates: &[Function],
    calldata: &[u8],
    client: &Arc<dyn BlobClient>,
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

    let block_infos = parse_commit_block_info(
        protocol_versioning,
        &new_blocks_data,
        l1_block_number,
        client,
    )
    .await?;
    Ok(block_infos)
}

async fn parse_commit_block_info(
    protocol_versioning: &ProtocolVersioning,
    data: &ethabi::Token,
    l1_block_number: u64,
    client: &Arc<dyn BlobClient>,
) -> Result<Vec<CommitBlock>, ParseError> {
    let ethabi::Token::Array(data) = data else {
        return Err(ParseError::InvalidCommitBlockInfo(
            "cannot convert newBlocksData to array".to_string(),
        ));
    };

    let mut result = vec![];
    for d in data {
        let (boojum_block, blob_block) = match protocol_versioning {
            ProtocolVersioning::OnlyV3 => (&0u64, &0u64),
            ProtocolVersioning::AllVersions {
                v2_start_batch_number,
                v3_start_batch_number,
                ..
            } => (v2_start_batch_number, v3_start_batch_number),
        };
        let commit_block = {
            if l1_block_number >= *blob_block {
                CommitBlock::try_from_token_resolve(d, client).await?
            } else if l1_block_number >= *boojum_block {
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
    use std::{collections::HashMap, num::NonZero, str::FromStr, sync::Arc};

    use tempfile::TempDir;
    use zksync_basic_types::{url::SensitiveUrl, L1BatchNumber, L2BlockNumber, H256, U64};
    use zksync_dal::{ConnectionPool, Core, CoreDal};
    use zksync_types::snapshots::SnapshotRecoveryStatus;
    use zksync_utils::{bytecode::hash_bytecode, env::Workspace};
    use zksync_web3_decl::client::{Client, DynClient, L1};

    use crate::{
        l1_fetcher::{
            blob_http_client::{BlobClient, BlobHttpClient, LocalDbBlobSource},
            constants::{
                local_diamond_proxy_addr, sepolia_diamond_proxy_addr, sepolia_initial_state_path,
                sepolia_versioning,
            },
            l1_fetcher::{L1Fetcher, L1FetcherConfig, ProtocolVersioning::OnlyV3},
        },
        processor::{
            genesis::{get_genesis_factory_deps, get_genesis_state},
            snapshot::StateCompressor,
            tree::TreeProcessor,
        },
    };

    fn sepolia_l1_client() -> Box<DynClient<L1>> {
        let url = SensitiveUrl::from_str(&"https://ethereum-sepolia-rpc.publicnode.com").unwrap();
        let eth_client = Client::http(url)
            .unwrap()
            .with_allowed_requests_per_second(NonZero::new(10).unwrap())
            .build();
        Box::new(eth_client)
    }

    pub fn local_l1_client() -> Box<DynClient<L1>> {
        let url = SensitiveUrl::from_str(&"http://127.0.0.1:8545").unwrap();
        let eth_client = Client::http(url).unwrap().build();
        Box::new(eth_client)
    }
    fn sepolia_l1_fetcher() -> L1Fetcher {
        let config = L1FetcherConfig {
            block_step: 10000,
            diamond_proxy_addr: sepolia_diamond_proxy_addr().parse().unwrap(),
            versioning: sepolia_versioning(),
        };
        L1Fetcher::new(
            config,
            sepolia_l1_client(),
            Arc::new(BlobHttpClient::new("https://api.sepolia.blobscan.com/blobs/").unwrap()),
        )
        .unwrap()
    }
    #[test_log::test(tokio::test)]
    async fn uncompressing_factory_deps_from_l2_to_l1_messages() {
        let eth_client = sepolia_l1_client();
        // commitBatch no. 403 from boojnet
        let tx = L1Fetcher::get_transaction_by_hash(
            &eth_client,
            H256::from_str("0x0624f45cf7ad6c3fe12c8c1d320ecdbd051ce0a6b79888c5754ee6f5b31d9ae6")
                .unwrap(),
        )
        .await
        .unwrap();
        let blob_provider: Arc<dyn BlobClient> =
            Arc::new(BlobHttpClient::new("https://api.sepolia.blobscan.com/blobs/").unwrap());
        let functions = L1Fetcher::commit_functions().unwrap();
        let blocks =
            L1Fetcher::process_tx_data(&functions, &blob_provider, tx, &sepolia_versioning())
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

        let eth_client = sepolia_l1_client();
        // commitBatch no. 11944 from boojnet, pubdata submitted using blobs
        let tx = L1Fetcher::get_transaction_by_hash(
            &eth_client,
            H256::from_str("0xd3778819cfa599e06b7d24fceff81de273c16a2cd2a1556e0ab268545acbe3ae")
                .unwrap(),
        )
        .await
        .unwrap();
        let blob_provider: Arc<dyn BlobClient> =
            Arc::new(BlobHttpClient::new("https://api.sepolia.blobscan.com/blobs/").unwrap());
        let functions = L1Fetcher::commit_functions().unwrap();
        let blocks =
            L1Fetcher::process_tx_data(&functions, &blob_provider, tx, &sepolia_versioning())
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
    async fn test_recovery_without_initial_state_file() {
        get_genesis_factory_deps();
        get_genesis_state();
        //TODO compare those with actual expected values
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
            .process_genesis_state(Some(sepolia_initial_state_path()))
            .await;
        processor.process_blocks(blocks).await;

        assert_eq!(34, processor.export_factory_deps().await.len());

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
    }
}

use std::{cmp, cmp::PartialEq, collections::HashMap, future::Future, sync::Arc};

use anyhow::{anyhow, Result};
use ethabi::{Contract, Event, Function};
use rand::random;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    sync::watch,
    time::{sleep, Duration},
};
use zksync_basic_types::{
    bytecode::BytecodeHash,
    protocol_version::{L1VerifierConfig, ProtocolSemanticVersion},
    web3::{BlockId, BlockNumber, FilterBuilder, Log, Transaction},
    Address, L1BatchNumber, PriorityOpId, H256, U256, U64,
};
use zksync_contracts::{hyperchain_contract, BaseSystemContractsHashes};
use zksync_eth_client::{CallFunctionArgs, ClientError, EthInterface};
use zksync_l1_contract_interface::i_executor::structures::StoredBatchInfo;
use zksync_object_store::{serialize_using_bincode, Bucket, ObjectStore, StoredObject};
use zksync_types::{l1::L1Tx, ProtocolVersion};
use zksync_web3_decl::{
    client::{DynClient, L1},
    error::EnrichedClientResult,
};

use crate::l1_fetcher::{
    blob_http_client::BlobClient,
    types::{v1::V1, v2::V2, CommitBlock, ParseError},
};

#[derive(Serialize, Deserialize)]
struct CachedCommitBlocks {
    blocks: Vec<CommitBlock>,
}

impl StoredObject for CachedCommitBlocks {
    const BUCKET: Bucket = Bucket::CommitBlocksCache;
    type Key<'a> = H256;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("commit_block_{}.bin", hex::encode(key))
    }

    serialize_using_bincode!();
}

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
    config: L1FetcherConfig,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RollupEventType {
    Commit,
    #[allow(unused)]
    Prove,
    Execute,
    NewPriorityTx,
}

pub enum RollupEventsFilter {
    L1BatchNumber(L1BatchNumber),
    PriorityOpId(PriorityOpId),
    None,
}

impl L1Fetcher {
    pub fn new(config: L1FetcherConfig, eth_client: Box<DynClient<L1>>) -> Result<Self> {
        tracing::info!("Initialized L1 fetcher with config: {config:?}");
        Ok(L1Fetcher { eth_client, config })
    }

    fn hyperchain_contract() -> Contract {
        hyperchain_contract()
    }

    fn commit_functions() -> Result<Vec<Function>> {
        let contract = Self::hyperchain_contract();
        Ok(vec![
            contract.functions_by_name("commitBatches").unwrap()[0].clone(),
            contract
                .functions_by_name("commitBatchesSharedBridge")
                .unwrap()[0]
                .clone(),
        ])
    }

    fn rollup_event_by_type(event_type: RollupEventType) -> Result<Event> {
        let event_name = match event_type {
            RollupEventType::Commit => "BlockCommit",
            RollupEventType::Prove => "BlocksVerification",
            RollupEventType::Execute => "BlockExecution",
            RollupEventType::NewPriorityTx => "NewPriorityRequest",
        };
        Ok(Self::hyperchain_contract().events_by_name(event_name)?[0].clone())
    }

    fn extract_l1_batch_number_from_rollup_event_log(
        event_type: RollupEventType,
        log: &Log,
    ) -> L1BatchNumber {
        let topic_index = match event_type {
            RollupEventType::Commit => 1,
            RollupEventType::Prove => 2,
            RollupEventType::Execute => 1,
            _ => panic!("{event_type:?} event doesn't have l1_batch_number"),
        };
        L1BatchNumber(log.topics[topic_index].to_low_u64_be() as u32)
    }

    fn extract_priority_op_id_from_rollup_event_log(
        event_type: RollupEventType,
        log: &Log,
    ) -> PriorityOpId {
        assert_eq!(event_type, RollupEventType::NewPriorityTx);
        L1Tx::try_from(log.clone()).unwrap().common_data.serial_id
    }

    pub async fn find_block_near_genesis(&self) -> U64 {
        let mut start_block: U64 = U64::zero();
        let last_l1_block_number = L1Fetcher::get_last_l1_block_number(&self.eth_client)
            .await
            .unwrap();
        let mut end_block = last_l1_block_number;
        // we use step override as such big step value guarantees that we don't miss
        // events on non-local networks
        let step = 50_000;
        while start_block + 2 * step < end_block {
            let mid = (start_block + end_block) / 2;
            let logs = self
                .get_logs(mid, mid + step - 1, RollupEventType::Commit)
                .await;
            if !logs.is_empty() {
                end_block = mid;
            } else {
                start_block = mid;
            }
        }
        end_block = last_l1_block_number;
        let mut current_block = start_block;
        loop {
            let filter_to_block = cmp::min(current_block + step - 1, end_block);
            let logs = self
                .get_logs(
                    current_block,
                    filter_to_block,
                    RollupEventType::NewPriorityTx,
                )
                .await;
            if !logs.is_empty() {
                if L1Fetcher::extract_priority_op_id_from_rollup_event_log(
                    RollupEventType::NewPriorityTx,
                    &logs[0],
                ) == PriorityOpId(0)
                {
                    return logs[0].block_number.unwrap();
                } else {
                    // The binary search didn't work, it's fine, we just return first block
                    return U64::zero();
                }
            }
            if filter_to_block == end_block {
                // The binary search didn't work, it's fine, we just return first block
                return U64::zero();
            }
            current_block = filter_to_block;
        }
    }

    pub async fn get_all_blocks_to_process(
        &self,
        blob_client: &Arc<dyn BlobClient>,
        commit_blocks_cache: Option<&Arc<dyn ObjectStore>>,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Vec<CommitBlock> {
        let end_block = L1Fetcher::get_last_l1_block_number(&self.eth_client)
            .await
            .unwrap();
        self.get_blocks_to_process(blob_client, end_block, commit_blocks_cache, stop_receiver)
            .await
    }

    pub async fn get_genesis_root_hash(&self) -> H256 {
        let genesis_block = self.find_block_near_genesis().await;
        let logs = self
            .get_logs(
                genesis_block,
                genesis_block + self.config.block_step,
                RollupEventType::Commit,
            )
            .await;
        let tx =
            L1Fetcher::get_transaction_by_hash(&self.eth_client, logs[0].transaction_hash.unwrap())
                .await
                .unwrap();
        parse_last_committed_l1_batch(&Self::commit_functions().unwrap(), &tx.input.0)
            .await
            .unwrap()
            .batch_hash
    }

    pub async fn get_stored_block_info(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<StoredBatchInfo> {
        let eth_block = self
            .get_latest_rollup_event(
                RollupEventType::Commit,
                RollupEventsFilter::L1BatchNumber(l1_batch_number + 1),
            )
            .await
            .unwrap()
            .block_number
            .unwrap();
        let logs = self
            .get_logs(eth_block, eth_block, RollupEventType::Commit)
            .await;
        for log in logs {
            let log_l1_batch_number =
                Self::extract_l1_batch_number_from_rollup_event_log(RollupEventType::Commit, &log);
            if l1_batch_number + 1 != log_l1_batch_number {
                continue;
            }
            let hash = log.transaction_hash.unwrap();
            let tx = L1Fetcher::get_transaction_by_hash(&self.eth_client, hash)
                .await
                .unwrap();
            return Ok(parse_last_committed_l1_batch(
                &Self::commit_functions().unwrap(),
                &tx.input.0,
            )
            .await
            .unwrap());
        }
        unreachable!("No logs found for block {}", l1_batch_number);
    }
    pub async fn get_latest_protocol_version(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> ProtocolVersion {
        let block_to_check = self
            .get_latest_rollup_event(
                RollupEventType::Execute,
                RollupEventsFilter::L1BatchNumber(l1_batch_number),
            )
            .await
            .unwrap()
            .block_number
            .unwrap();
        let bootloader_bytecode_hash: H256 =
            CallFunctionArgs::new("getL2BootloaderBytecodeHash", ())
                .with_block(BlockId::Number(BlockNumber::Number(block_to_check)))
                .for_contract(self.config.diamond_proxy_addr, &Self::hyperchain_contract())
                .call(&self.eth_client)
                .await
                .unwrap();
        let default_aa_bytecode_hash: H256 =
            CallFunctionArgs::new("getL2DefaultAccountBytecodeHash", ())
                .with_block(BlockId::Number(BlockNumber::Number(block_to_check)))
                .for_contract(self.config.diamond_proxy_addr, &Self::hyperchain_contract())
                .call(&self.eth_client)
                .await
                .unwrap();
        let packed_protocol_version: U256 = CallFunctionArgs::new("getProtocolVersion", ())
            .with_block(BlockId::Number(BlockNumber::Number(block_to_check)))
            .for_contract(self.config.diamond_proxy_addr, &Self::hyperchain_contract())
            .call(&self.eth_client)
            .await
            .unwrap();

        ProtocolVersion {
            version: ProtocolSemanticVersion::try_from_packed(packed_protocol_version).unwrap(),
            timestamp: 0,
            l1_verifier_config: L1VerifierConfig::default(),
            base_system_contracts_hashes: BaseSystemContractsHashes {
                bootloader: bootloader_bytecode_hash,
                default_aa: default_aa_bytecode_hash,
                evm_emulator: None,
            },
            tx: None,
        }
    }

    #[async_recursion::async_recursion]
    async fn get_events_inner(
        &self,
        from: BlockNumber,
        to: BlockNumber,
        topics1: Option<Vec<H256>>,
        topics2: Option<Vec<H256>>,
        addresses: Option<Vec<Address>>,
        retries_left: usize,
    ) -> EnrichedClientResult<Vec<Log>> {
        pub const RETRY_LIMIT: usize = 5;
        const TOO_MANY_RESULTS_INFURA: &str = "query returned more than";
        const TOO_MANY_RESULTS_ALCHEMY: &str = "response size exceeded";
        const TOO_MANY_RESULTS_RETH: &str = "length limit exceeded";
        const TOO_BIG_RANGE_RETH: &str = "query exceeds max block range";
        const TOO_MANY_RESULTS_CHAINSTACK: &str = "range limit exceeded";

        let mut builder = FilterBuilder::default()
            .from_block(from)
            .to_block(to)
            .topics(topics1.clone(), topics2.clone(), None, None);
        if let Some(addresses) = addresses.clone() {
            builder = builder.address(addresses);
        }
        let filter = builder.build();
        let mut result = self.eth_client.logs(&filter).await;

        // This code is compatible with both Infura and Alchemy API providers.
        // Note: we don't handle rate-limits here - assumption is that we're never going to hit them.
        if let Err(err) = &result {
            tracing::warn!("Provider returned error message: {err}");
            let err_message = err.as_ref().to_string();
            let err_code = if let ClientError::Call(err) = err.as_ref() {
                Some(err.code())
            } else {
                None
            };

            let should_retry = |err_code, err_message: String| {
                // All of these can be emitted by either API provider.
                err_code == Some(-32603)             // Internal error
                    || err_message.contains("failed")    // Server error
                    || err_message.contains("timed out") // Time-out error
            };

            // check whether the error is related to having too many results
            if err_message.contains(TOO_MANY_RESULTS_INFURA)
                || err_message.contains(TOO_MANY_RESULTS_ALCHEMY)
                || err_message.contains(TOO_MANY_RESULTS_RETH)
                || err_message.contains(TOO_BIG_RANGE_RETH)
                || err_message.contains(TOO_MANY_RESULTS_CHAINSTACK)
            {
                // get the numeric block ids
                let from_number = match from {
                    BlockNumber::Number(num) => num,
                    _ => {
                        // invalid variant
                        return result;
                    }
                };
                let to_number = match to {
                    BlockNumber::Number(num) => num,
                    BlockNumber::Latest => self.eth_client.block_number().await?,
                    _ => {
                        // invalid variant
                        return result;
                    }
                };

                // divide range into two halves and recursively fetch them
                let mid = (from_number + to_number) / 2;

                // safety check to prevent infinite recursion (quite unlikely)
                if from_number >= mid {
                    tracing::warn!("Infinite recursion detected while getting events: from_number={from_number:?}, mid={mid:?}");
                    return result;
                }

                tracing::warn!("Splitting block range in half: {from:?} - {mid:?} - {to:?}");
                let mut first_half = self
                    .get_events_inner(
                        from,
                        BlockNumber::Number(mid),
                        topics1.clone(),
                        topics2.clone(),
                        addresses.clone(),
                        RETRY_LIMIT,
                    )
                    .await?;
                let mut second_half = self
                    .get_events_inner(
                        BlockNumber::Number(mid + 1u64),
                        to,
                        topics1,
                        topics2,
                        addresses,
                        RETRY_LIMIT,
                    )
                    .await?;

                first_half.append(&mut second_half);
                result = Ok(first_half);
            } else if should_retry(err_code, err_message) && retries_left > 0 {
                tracing::warn!("Retrying. Retries left: {retries_left}");
                result = self
                    .get_events_inner(from, to, topics1, topics2, addresses, retries_left - 1)
                    .await;
            }
        }

        result
    }

    async fn get_logs(
        &self,
        start_block: U64,
        end_block: U64,
        rollup_event_type: RollupEventType,
    ) -> Vec<Log> {
        let event = L1Fetcher::rollup_event_by_type(rollup_event_type).unwrap();
        L1Fetcher::retry_call(
            || {
                self.get_events_inner(
                    BlockNumber::Number(start_block),
                    BlockNumber::Number(end_block),
                    Some(vec![event.signature()]),
                    None,
                    Some(vec![self.config.diamond_proxy_addr]),
                    5,
                )
            },
            L1FetchError::GetLogs,
        )
        .await
        .unwrap()
    }

    pub async fn get_last_executed_l1_batch_number(&self) -> Option<L1BatchNumber> {
        let log = self
            .get_latest_rollup_event(RollupEventType::Execute, RollupEventsFilter::None)
            .await
            .unwrap();
        Some(Self::extract_l1_batch_number_from_rollup_event_log(
            RollupEventType::Execute,
            &log,
        ))
    }

    pub async fn get_latest_rollup_event(
        &self,
        rollup_event_type: RollupEventType,
        filter: RollupEventsFilter,
    ) -> Option<Log> {
        let end_block = L1Fetcher::get_last_l1_block_number(&self.eth_client)
            .await
            .unwrap();
        let mut current_block = end_block;
        loop {
            let filter_from_block = current_block
                .checked_sub(U64::from(self.config.block_step - 1))
                .unwrap_or_default();
            tracing::info!("checking {} - {}", filter_from_block, current_block);
            let mut logs = self
                .get_logs(filter_from_block, current_block, rollup_event_type)
                .await;
            logs.reverse();
            for log in logs {
                match filter {
                    RollupEventsFilter::L1BatchNumber(l1_batch_number) => {
                        let log_batch_number = Self::extract_l1_batch_number_from_rollup_event_log(
                            rollup_event_type,
                            &log,
                        );
                        if l1_batch_number == log_batch_number {
                            return Some(log);
                        } else {
                            continue;
                        }
                    }
                    RollupEventsFilter::PriorityOpId(priority_op_id) => {
                        let log_priority_op_id = Self::extract_priority_op_id_from_rollup_event_log(
                            rollup_event_type,
                            &log,
                        );
                        if log_priority_op_id == priority_op_id {
                            return Some(log);
                        } else {
                            continue;
                        }
                    }
                    RollupEventsFilter::None => {
                        return Some(log);
                    }
                }
            }
            if filter_from_block == U64::zero() {
                return None;
            }
            current_block = filter_from_block - 1;
        }
    }

    pub async fn get_last_processed_priority_transaction(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> L1Tx {
        let block_just_after_execute_tx_eth_block = self
            .get_latest_rollup_event(
                RollupEventType::Execute,
                RollupEventsFilter::L1BatchNumber(l1_batch_number),
            )
            .await
            .unwrap()
            .block_number
            .unwrap()
            + 1;
        tracing::info!(
            "Found execute event for block {l1_batch_number} on eth block {block_just_after_execute_tx_eth_block}"
        );

        let first_unprocessed_priority_id: U256 =
            CallFunctionArgs::new("getFirstUnprocessedPriorityTx", ())
                .with_block(BlockId::Number(BlockNumber::Number(
                    block_just_after_execute_tx_eth_block,
                )))
                .for_contract(self.config.diamond_proxy_addr, &Self::hyperchain_contract())
                .call(&self.eth_client)
                .await
                .unwrap();
        let first_unprocessed_priority_id = PriorityOpId(first_unprocessed_priority_id.as_u64());

        tracing::info!(
            "First unprocessed priority tx id: {:?}",
            first_unprocessed_priority_id
        );

        let log = self
            .get_latest_rollup_event(
                RollupEventType::NewPriorityTx,
                RollupEventsFilter::PriorityOpId(first_unprocessed_priority_id - 1),
            )
            .await
            .unwrap_or_else(|| {
                panic!(
                    "Unable to find priority tx with id {}",
                    first_unprocessed_priority_id - 1
                )
            });
        L1Tx::try_from(log).unwrap()
    }

    async fn fetch_priority_txs(&self, start_block: U64, end_block: U64) -> Vec<L1Tx> {
        let logs = self
            .get_logs(start_block, end_block, RollupEventType::NewPriorityTx)
            .await;
        logs.iter()
            .map(|log| L1Tx::try_from(log.clone()).unwrap())
            .collect()
    }

    pub async fn get_blocks_to_process(
        &self,
        blob_client: &Arc<dyn BlobClient>,
        end_block: U64,
        commit_blocks_cache: Option<&Arc<dyn ObjectStore>>,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Vec<CommitBlock> {
        let start_block = self.find_block_near_genesis().await;

        let functions = L1Fetcher::commit_functions().unwrap();
        let block_step = self.config.block_step;

        let mut current_block = start_block;
        let mut result: Vec<CommitBlock> = vec![];
        let mut priority_txs = vec![];
        let mut priority_txs_so_far = 0;
        let mut last_processed_priority_tx = 0;
        let mut factory_deps_hashes = HashMap::new();
        let last_executed_batch = self
            .get_last_executed_l1_batch_number()
            .await
            .expect("no executed batches found on L1");

        loop {
            let filter_to_block = cmp::min(current_block + block_step - 1, end_block);
            priority_txs.extend(
                self.fetch_priority_txs(current_block, filter_to_block)
                    .await,
            );

            // Grab all relevant logs.
            let logs = self
                .get_logs(current_block, filter_to_block, RollupEventType::Commit)
                .await;
            tracing::info!(
                "Found {} committed blocks and {} priority txs for blocks: {current_block}-{filter_to_block}",
                logs.len(), priority_txs.len() - priority_txs_so_far
            );
            priority_txs_so_far = priority_txs.len();

            for log in logs {
                if *stop_receiver.borrow() {
                    panic!("Stop requested");
                }
                let hash = log.transaction_hash.unwrap();
                let commitment: H256 = H256::from(log.topics[3].to_fixed_bytes());
                let mut cached_object: Option<CachedCommitBlocks> = None;
                if let Some(commit_blocks_cache) = commit_blocks_cache.as_ref() {
                    cached_object = commit_blocks_cache.get(commitment).await.ok();
                }

                let blocks = if let Some(cached_objects) = cached_object {
                    cached_objects.blocks
                } else {
                    let l1_batch_number = H256::from(log.topics[1].to_fixed_bytes());
                    if l1_batch_number.to_low_u64_be() as u32 > last_executed_batch.0 {
                        continue;
                    }
                    let tx = L1Fetcher::get_transaction_by_hash(&self.eth_client, hash)
                        .await
                        .unwrap();
                    let blocks = L1Fetcher::process_tx_data(
                        &functions,
                        blob_client,
                        tx,
                        &self.config.versioning,
                    )
                    .await
                    .unwrap();

                    let cached_blocks = CachedCommitBlocks { blocks };
                    if let Some(commit_blocks_cache) = commit_blocks_cache.as_ref() {
                        commit_blocks_cache
                            .put(commitment, &cached_blocks)
                            .await
                            .unwrap();
                    }
                    cached_blocks.blocks
                };

                for mut block in blocks {
                    while !result.is_empty()
                        && block.l1_batch_number <= result.last().unwrap().l1_batch_number
                    {
                        let rollbacked_block = result.last().unwrap();
                        tracing::warn!(
                            "Removing block {} because of a detected rollback",
                            rollbacked_block.l1_batch_number
                        );
                        for factory_dep in &rollbacked_block.factory_deps {
                            let hashed = BytecodeHash::for_bytecode(factory_dep).value();
                            factory_deps_hashes.remove(&hashed);
                        }
                        last_processed_priority_tx -= rollbacked_block.l1_tx_count as usize;
                        result.pop();
                    }
                    assert_eq!(result.len() as u64, block.l1_batch_number - 1);
                    for _ in 0..block.l1_tx_count {
                        let priority_tx = priority_txs[last_processed_priority_tx].clone();
                        block
                            .priority_ops_onchain_data
                            .push(priority_tx.common_data.onchain_data());
                        for factory_dep in &priority_tx.execute.factory_deps {
                            let hashed = BytecodeHash::for_bytecode(factory_dep).value();
                            if let std::collections::hash_map::Entry::Vacant(e) =
                                factory_deps_hashes.entry(hashed)
                            {
                                e.insert(());
                                block.factory_deps.push(factory_dep.clone());
                            } else {
                                continue;
                            }
                        }
                        last_processed_priority_tx += 1;
                    }
                    block.commitment = commitment;
                    if block.l1_batch_number % 100 == 0 {
                        tracing::info!("Finished processing l1_batch number {}, last processed priority tx: {}",
                            block.l1_batch_number, last_processed_priority_tx - 1);
                    }
                    result.push(block)
                }
            }

            if filter_to_block == end_block {
                if let Some(last_block) = result.last() {
                    tracing::info!(
                        "Finished processing L1 data, last processed l1 batch: {}, \
                    last processed priority tx id: {}",
                        last_block.l1_batch_number,
                        last_processed_priority_tx - 1
                    );
                }
                break;
            }
            current_block = filter_to_block + 1;
        }
        result
    }

    async fn process_tx_data(
        commit_functions: &[Function],
        blob_client: &Arc<dyn BlobClient>,
        tx: Transaction,
        protocol_versioning: &ProtocolVersioning,
    ) -> Result<Vec<CommitBlock>, ParseError> {
        let block_number = tx.block_number.unwrap().as_u64();
        match parse_calldata(
            protocol_versioning,
            block_number,
            commit_functions,
            &tx.input.0,
            blob_client,
        )
        .await
        {
            Ok(blks) => Ok(blks),
            Err(e) => {
                match e.clone() {
                    ParseError::BlobStorageError(err) => {
                        tracing::error!("Blob storage error {err}");
                    }
                    ParseError::BlobFormatError(data, inner) => {
                        tracing::error!("Cannot parse {}: {}", data, inner);
                    }
                    _ => {
                        tracing::error!(
                            "Failed to parse calldata: {e}, encountered on block: {block_number}"
                        );
                    }
                }
                Err(e)
            }
        }
    }

    /// Get a specified transaction on L1 by its hash.
    #[allow(clippy::borrowed_box)]
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

    #[allow(clippy::borrowed_box)]
    fn query_client(eth_client: &Box<DynClient<L1>>) -> &dyn EthInterface {
        eth_client
    }
    /// Get the last published L1 block.
    #[allow(clippy::borrowed_box)]
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
        sleep(Duration::from_millis(100)).await;
        for attempt in 1..=100 {
            match callback().await {
                Ok(x) => return Ok(x),
                Err(e) => {
                    tracing::error!("attempt {attempt}: failed to fetch from L1: {e}");
                    sleep(Duration::from_millis(
                        attempt * 1000 + random::<u64>() % 500,
                    ))
                    .await;
                }
            }
        }
        Err(err.into())
    }
}

pub async fn parse_last_committed_l1_batch(
    commit_candidates: &[Function],
    calldata: &[u8],
) -> Result<StoredBatchInfo, ParseError> {
    let commit_fn = commit_candidates
        .iter()
        .find(|f| f.short_signature() == calldata[..4])
        .unwrap();
    let mut parsed_input = commit_fn.decode_input(&calldata[4..]).unwrap();

    let _new_blocks_data = parsed_input.pop().unwrap();
    let stored_block_info = parsed_input.pop().unwrap();
    Ok(StoredBatchInfo::from_token(stored_block_info).unwrap())
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
    use std::{str::FromStr, sync::Arc};

    use tempfile::TempDir;
    use tokio::sync::watch;
    use zksync_basic_types::{bytecode::BytecodeHash, H256, U64};
    use zksync_utils::env::Workspace;

    use crate::{
        l1_fetcher::{
            blob_http_client::{BlobClient, BlobHttpClient},
            constants::{
                sepolia_blob_client, sepolia_initial_state_path, sepolia_l1_client,
                sepolia_l1_fetcher, sepolia_versioning,
            },
            fetcher::L1Fetcher,
        },
        processor::{snapshot::StateCompressor, tree::TreeProcessor},
    };

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
            BytecodeHash::for_bytecode(&block.factory_deps[0]).value(),
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
            BytecodeHash::for_bytecode(&block.factory_deps[0]).value(),
            H256::from_str("0x0100009ffa898e7aa96817a2267079a0c8e92ce719b16b2be4cb17a4db04b0e2")
                .unwrap()
        );
    }

    #[test_log::test(tokio::test)]
    async fn get_blocks_to_process_works_correctly() {
        let (_, stop_receiver) = watch::channel(false);
        let l1_fetcher = sepolia_l1_fetcher();
        // batches up to 109
        let blocks = l1_fetcher
            .get_blocks_to_process(
                &sepolia_blob_client(),
                U64::from(4820508),
                None,
                &stop_receiver,
            )
            .await;

        // blocks from up 109, two blocks were sent twice (precisely ...80,81,82,83,82,83,84...)
        // we expect no duplicates in result
        assert_eq!(blocks.len(), 109);
    }

    #[test_log::test(tokio::test)]
    async fn get_genesis_block_works_correctly_for_sepolia() {
        let l1_fetcher = sepolia_l1_fetcher();
        let block = l1_fetcher.find_block_near_genesis().await;
        assert_eq!(block, U64::from(4800177));
    }

    #[test_log::test(tokio::test)]
    async fn get_genesis_root_hash_works_correctly_for_sepolia() {
        let l1_fetcher = sepolia_l1_fetcher();
        let genesis_hash = l1_fetcher.get_genesis_root_hash().await;
        assert_eq!(
            "0xd2892ccfb454d0cfa21f9c769fcbbc8da7a8fc9faf8c597b9bdfffc1e5adb0f2",
            format!("{:?}", genesis_hash)
        );
    }

    #[test_log::test(tokio::test)]
    async fn test_process_blocks() {
        let (_, stop_receiver) = watch::channel(false);
        let temp_dir = TempDir::new().unwrap().into_path().join("db");

        let l1_fetcher = sepolia_l1_fetcher();
        // batches up to batch 110, we want that many to test whether "duplicated" batches don't break anything
        let blocks = l1_fetcher
            .get_blocks_to_process(
                &sepolia_blob_client(),
                U64::from(4820533),
                None,
                &stop_receiver,
            )
            .await;

        let mut processor = StateCompressor::new(temp_dir).await;
        processor
            .process_genesis_state(sepolia_initial_state_path())
            .await;
        processor.process_blocks(blocks, &stop_receiver).await;

        // https://sepolia.explorer.zksync.io/block/671
        let miniblock = processor.read_latest_miniblock_metadata();
        assert_eq!(671, miniblock.number);
        assert_eq!(1701692001, miniblock.timestamp);
        assert_eq!(
            "0x548c9e9715b00b0f48cede784228a9f8e3e80196c73d73c77afaef1b827b98de",
            format!("{:?}", miniblock.hash)
        );

        assert_eq!(34, processor.export_factory_deps().await.len());

        let temp_dir2 = TempDir::new().unwrap().into_path().join("db2");
        let mut reconstructed = TreeProcessor::new(temp_dir2).await.unwrap();
        reconstructed
            .process_snapshot_storage_logs(processor.export_storage_logs().await)
            .await
            .unwrap();
        // taken from https://sepolia.explorer.zksync.io/batch/109
        assert_eq!(
            "0x2673516079255c6aad7834c46e34a5b58183218a975ab30cfba175faa3c2d5f1",
            format!("{:?}", reconstructed.get_root_hash())
        );
    }
}

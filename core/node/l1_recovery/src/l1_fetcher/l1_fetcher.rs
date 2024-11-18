use std::{cmp, cmp::PartialEq, collections::HashMap, fs::File, future::Future, sync::Arc};

use anyhow::{anyhow, Result};
use ethabi::{Contract, Event, Function};
use rand::random;
use thiserror::Error;
use tokio::time::{sleep, Duration};
use zksync_basic_types::{
    bytecode::BytecodeHash,
    protocol_version::{L1VerifierConfig, ProtocolSemanticVersion},
    web3::{contract::Tokenizable, BlockId, BlockNumber, FilterBuilder, Log, Transaction},
    Address, L1BatchNumber, PriorityOpId, H256, U256, U64,
};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::eth_watcher_dal::EventType;
use zksync_eth_client::{CallFunctionArgs, EthInterface};
use zksync_l1_contract_interface::i_executor::structures::StoredBatchInfo;
use zksync_types::{l1::L1Tx, ProtocolVersion};
use zksync_utils::env::Workspace;
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
    config: L1FetcherConfig,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RollupEventType {
    Commit,
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
        Ok(L1Fetcher { eth_client, config })
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

    fn rollup_event_by_type(event_type: RollupEventType) -> Result<Event> {
        let event_name = match event_type {
            RollupEventType::Commit => "BlockCommit",
            RollupEventType::Prove => "BlocksVerification",
            RollupEventType::Execute => "BlockExecution",
            RollupEventType::NewPriorityTx => "NewPriorityRequest",
        };
        Ok(L1Fetcher::v1_contract()?.events_by_name(event_name)?[0].clone())
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

    pub async fn get_all_blocks_to_process(
        &self,
        blob_client: &Arc<dyn BlobClient>,
    ) -> Vec<CommitBlock> {
        let start_block = U64::zero();
        let end_block = L1Fetcher::get_last_l1_block_number(&self.eth_client)
            .await
            .unwrap();
        self.get_blocks_to_process(blob_client, start_block, end_block)
            .await
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
            let tx = L1Fetcher::retry_call(
                || L1Fetcher::get_transaction_by_hash(&self.eth_client, hash),
                L1FetchError::GetTx,
            )
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
                .for_contract(
                    self.config.diamond_proxy_addr,
                    &Self::v1_contract().unwrap(),
                )
                .call(&self.eth_client)
                .await
                .unwrap();
        let default_aa_bytecode_hash: H256 =
            CallFunctionArgs::new("getL2DefaultAccountBytecodeHash", ())
                .with_block(BlockId::Number(BlockNumber::Number(U64::from(
                    block_to_check,
                ))))
                .for_contract(
                    self.config.diamond_proxy_addr,
                    &Self::v1_contract().unwrap(),
                )
                .call(&self.eth_client)
                .await
                .unwrap();
        let packed_protocol_version: U256 = CallFunctionArgs::new("getProtocolVersion", ())
            .with_block(BlockId::Number(BlockNumber::Number(U64::from(
                block_to_check,
            ))))
            .for_contract(
                self.config.diamond_proxy_addr,
                &Self::v1_contract().unwrap(),
            )
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

    async fn get_logs(
        &self,
        start_block: U64,
        end_block: U64,
        rollup_event_type: RollupEventType,
    ) -> Vec<Log> {
        assert!(end_block - start_block <= U64::from(self.config.block_step));
        let event = L1Fetcher::rollup_event_by_type(rollup_event_type).unwrap();
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
                .with_block(BlockId::Number(BlockNumber::Number(U64::from(
                    block_just_after_execute_tx_eth_block,
                ))))
                .for_contract(
                    self.config.diamond_proxy_addr,
                    &Self::v1_contract().unwrap(),
                )
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
            .expect(&format!(
                "Unable to find priority tx with id {}",
                first_unprocessed_priority_id - 1
            ));
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
        start_block: U64,
        end_block: U64,
    ) -> Vec<CommitBlock> {
        assert!(start_block <= end_block);

        let functions = L1Fetcher::commit_functions().unwrap();
        let block_step = self.config.block_step;

        let mut current_block = start_block;
        let mut result = vec![];
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
                let hash = log.transaction_hash.unwrap();
                let commitment: H256 = H256::from(log.topics[3].to_fixed_bytes());
                let l1_batch_number = H256::from(log.topics[1].to_fixed_bytes());
                if l1_batch_number.to_low_u64_be() as u32 > last_executed_batch.0 {
                    continue;
                }
                let tx = L1Fetcher::retry_call(
                    || L1Fetcher::get_transaction_by_hash(&self.eth_client, hash),
                    L1FetchError::GetTx,
                )
                .await
                .unwrap();
                let blocks = L1Fetcher::process_tx_data(
                    &functions,
                    &blob_client,
                    tx,
                    &self.config.versioning,
                )
                .await
                .unwrap();

                for mut block in blocks {
                    for _ in 0..block.l1_tx_count {
                        let priority_tx = priority_txs[last_processed_priority_tx].clone();
                        tracing::info!(
                            "Processing priority tx: {} with {} factory deps",
                            priority_tx.serial_id(),
                            priority_tx.execute.factory_deps.len()
                        );
                        block
                            .priority_ops_onchain_data
                            .push(priority_tx.common_data.onchain_data());
                        for factory_dep in &priority_tx.execute.factory_deps {
                            let hashed = BytecodeHash::for_bytecode(factory_dep).value();
                            if factory_deps_hashes.contains_key(&hashed) {
                                continue;
                            } else {
                                factory_deps_hashes.insert(hashed, ());
                                block.factory_deps.push(factory_dep.clone());
                            }
                        }
                        last_processed_priority_tx += 1;
                    }
                    block.commitment = commitment;
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
    return Ok(StoredBatchInfo::from_token(stored_block_info).unwrap());
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
    use std::{num::NonZero, str::FromStr, sync::Arc};

    use tempfile::TempDir;
    use zksync_basic_types::{
        bytecode::BytecodeHash, url::SensitiveUrl, web3::keccak256, L1BatchNumber, H256, U64,
    };
    use zksync_dal::{ConnectionPool, Core};
    use zksync_utils::env::Workspace;
    use zksync_web3_decl::client::{Client, DynClient, L1};

    use crate::{
        l1_fetcher::{
            blob_http_client::{BlobClient, BlobHttpClient, LocalStorageBlobSource},
            constants::{
                local_diamond_proxy_addr, local_initial_state_path, sepolia_diamond_proxy_addr,
                sepolia_initial_state_path, sepolia_versioning,
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

    fn sepolia_l1_fetcher() -> L1Fetcher {
        let config = L1FetcherConfig {
            block_step: 10000,
            diamond_proxy_addr: sepolia_diamond_proxy_addr().parse().unwrap(),
            versioning: sepolia_versioning(),
        };
        L1Fetcher::new(config, sepolia_l1_client()).unwrap()
    }

    fn sepolia_blob_client() -> Arc<dyn BlobClient> {
        Arc::new(BlobHttpClient::new("https://api.sepolia.blobscan.com/blobs/").unwrap())
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
        let l1_fetcher = sepolia_l1_fetcher();
        // batches from 10 to 109
        let blocks = l1_fetcher
            .get_blocks_to_process(
                &sepolia_blob_client(),
                U64::from(4801161),
                U64::from(4820508),
            )
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
        // batches up to batch 110, we want that many to test whether "duplicated" batches don't break anything
        let blocks = l1_fetcher
            .get_blocks_to_process(
                &sepolia_blob_client(),
                U64::from(4800000),
                U64::from(4820533),
            )
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
            "0x2673516079255c6aad7834c46e34a5b58183218a975ab30cfba175faa3c2d5f1",
            format!("{:?}", reconstructed.get_root_hash())
        );
        // https://sepolia.explorer.zksync.io/block/671
        let miniblock = reconstructed.read_latest_miniblock_metadata();
        assert_eq!(671, miniblock.number);
        assert_eq!(1701692001, miniblock.timestamp);
        assert_eq!(
            "0x548c9e9715b00b0f48cede784228a9f8e3e80196c73d73c77afaef1b827b98de",
            format!("{:?}", miniblock.hash)
        )
    }

    // #[test_log::test(tokio::test)]
    // async fn test_reconstruct_priority_data() {
    //     let connection_pool = ConnectionPool::<Core>::builder(
    //         "postgres://postgres:notsecurepassword@localhost:5432/zksync_server_localhost_era"
    //             .parse()
    //             .unwrap(),
    //         10,
    //     )
    //     .build()
    //     .await
    //     .unwrap();
    //     let url = SensitiveUrl::from_str(&"http://127.0.0.1:8545").unwrap();
    //     let eth_client = Client::http(url).unwrap().build();
    //     let blob_client: Arc<dyn BlobClient> =
    //         Arc::new(LocalStorageBlobSource::new(connection_pool.clone()));
    //     let fetcher = L1Fetcher::new(
    //         L1FetcherConfig {
    //             block_step: 100_000,
    //             diamond_proxy_addr: local_diamond_proxy_addr().parse().unwrap(),
    //             versioning: OnlyV3,
    //         },
    //         Box::new(eth_client),
    //     )
    //     .unwrap();
    //     let blocks = fetcher.get_all_blocks_to_process(&blob_client).await;
    //     tracing::info!("blocks len: {}", blocks.len());
    //     let block = blocks[37].clone();
    //     tracing::info!("{:?}", block);
    //
    //     let stored_block = fetcher
    //         .get_stored_block_info(L1BatchNumber(block.l1_batch_number as u32))
    //         .await
    //         .unwrap();
    //     tracing::info!("{:?}", stored_block);
    //     assert_eq!(
    //         block.rollup_last_leaf_index, stored_block.index_repeated_storage_changes,
    //         "index_repeated_storage_changes mismatch"
    //     );
    //     assert_eq!(
    //         block.priority_operations_hash, stored_block.priority_operations_hash,
    //         "priority_operations_hash mismatch"
    //     );
    //     assert_eq!(
    //         block.timestamp,
    //         stored_block.timestamp.as_u64(),
    //         "timestamp mismatch"
    //     );
    //     assert_eq!(
    //         block.l2_logs_tree_root, stored_block.l2_logs_tree_root,
    //         "l2_logs_tree_root mismatch"
    //     );
    //     assert_eq!(
    //         block.commitment, stored_block.commitment,
    //         "commitment mismatch"
    //     );
    //
    //     let mut rolling_hash: H256 = keccak256(&[]).into();
    //     for onchain_data in &block.priority_ops_onchain_data {
    //         let mut preimage = Vec::new();
    //         preimage.extend(rolling_hash.as_bytes());
    //         preimage.extend(onchain_data.onchain_data_hash.as_bytes());
    //
    //         rolling_hash = keccak256(&preimage).into();
    //     }
    //     assert_eq!(
    //         rolling_hash, stored_block.priority_operations_hash,
    //         "priority_operations_hash mismatch"
    //     );
    // }
}

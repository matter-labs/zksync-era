use zksync_system_constants::{L2_ETH_TOKEN_ADDRESS, TRANSFER_EVENT_TOPIC};
use zksync_types::{
    api::{ApiEthTransferEvents, L2ToL1Log, Log},
    web3::types::{Bytes, Index, U256, U64},
    Address, H256,
};

#[derive(sqlx::FromRow, Debug, Clone)]
pub struct StorageWeb3Log {
    pub address: Vec<u8>,
    pub topic1: Vec<u8>,
    pub topic2: Vec<u8>,
    pub topic3: Vec<u8>,
    pub topic4: Vec<u8>,
    pub value: Vec<u8>,
    pub block_hash: Option<Vec<u8>>,
    pub miniblock_number: i64,
    pub l1_batch_number: Option<i64>,
    pub tx_hash: Vec<u8>,
    pub tx_index_in_block: i32,
    pub event_index_in_block: i32,
    pub event_index_in_tx: i32,
    pub event_index_in_block_without_eth_transfer: i32,
    pub event_index_in_tx_without_eth_transfer: i32,
}

impl StorageWeb3Log {
    pub fn maybe_into_extended_storage_log(
        self,
        api_eth_transfer_events: ApiEthTransferEvents,
    ) -> Option<ExtendedStorageWeb3Log> {
        return if api_eth_transfer_events == ApiEthTransferEvents::Disabled
            && self.address == L2_ETH_TOKEN_ADDRESS.as_bytes()
            && self.topic1 == TRANSFER_EVENT_TOPIC.as_bytes()
        {
            None
        } else {
            Some(ExtendedStorageWeb3Log(
                self.clone(),
                api_eth_transfer_events,
            ))
        };
    }
}

struct ExtendedStorageWeb3Log(pub StorageWeb3Log, pub ApiEthTransferEvents);

impl From<ExtendedStorageWeb3Log> for Log {
    fn from(log_with_transfer_events_mode: ExtendedStorageWeb3Log) -> Log {
        let log = log_with_transfer_events_mode.0;
        let api_eth_transfer_events = log_with_transfer_events_mode.1;

        let event_index_in_block = match api_eth_transfer_events {
            ApiEthTransferEvents::Enabled => log.event_index_in_block,
            ApiEthTransferEvents::Disabled => {
                // reducing it by 1 here, because index 0 in DB means that events weren't migrated
                if log.event_index_in_block_without_eth_transfer > 0 {
                    log.event_index_in_block_without_eth_transfer - 1
                } else {
                    log.event_index_in_block_without_eth_transfer
                }
            }
        };
        let event_index_in_tx = match api_eth_transfer_events {
            ApiEthTransferEvents::Enabled => log.event_index_in_tx,
            ApiEthTransferEvents::Disabled => {
                // reducing it by 1 here, because index 0 in DB means that events weren't migrated
                if log.event_index_in_tx_without_eth_transfer > 0 {
                    log.event_index_in_tx_without_eth_transfer - 1
                } else {
                    log.event_index_in_tx_without_eth_transfer
                }
            }
        };

        let topics = vec![log.topic1, log.topic2, log.topic3, log.topic4]
            .into_iter()
            .filter_map(|topic| {
                if !topic.is_empty() {
                    Some(H256::from_slice(&topic))
                } else {
                    None
                }
            })
            .collect();

        Log {
            address: Address::from_slice(&log.address),
            topics,
            data: Bytes(log.value),
            block_hash: log.block_hash.map(|hash| H256::from_slice(&hash)),
            block_number: Some(U64::from(log.miniblock_number as u32)),
            l1_batch_number: log.l1_batch_number.map(U64::from),
            transaction_hash: Some(H256::from_slice(&log.tx_hash)),
            transaction_index: Some(Index::from(log.tx_index_in_block as u32)),
            log_index: Some(U256::from(event_index_in_block as u32)),
            transaction_log_index: Some(U256::from(event_index_in_block as u32)),
            log_type: None,
            removed: Some(false),
        }
    }
}

#[derive(sqlx::FromRow, Debug, Clone)]
pub struct StorageL2ToL1Log {
    pub block_hash: Option<Vec<u8>>,
    pub miniblock_number: i64,
    pub l1_batch_number: Option<i64>,
    pub log_index_in_miniblock: i32,
    pub log_index_in_tx: i32,
    pub tx_hash: Vec<u8>,
    pub shard_id: i32,
    pub is_service: bool,
    pub tx_index_in_miniblock: i32,
    pub tx_index_in_l1_batch: i32,
    pub sender: Vec<u8>,
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

impl From<StorageL2ToL1Log> for L2ToL1Log {
    fn from(log: StorageL2ToL1Log) -> L2ToL1Log {
        L2ToL1Log {
            block_hash: log.block_hash.map(|hash| H256::from_slice(&hash)),
            block_number: (log.miniblock_number as u32).into(),
            l1_batch_number: (log.l1_batch_number).map(|n| (n as u32).into()),
            log_index: (log.log_index_in_miniblock as u32).into(),
            transaction_index: (log.tx_index_in_miniblock as u32).into(),
            transaction_hash: H256::from_slice(&log.tx_hash),
            transaction_log_index: (log.log_index_in_tx as u32).into(),
            shard_id: (log.shard_id as u32).into(),
            is_service: log.is_service,
            sender: Address::from_slice(&log.sender),
            tx_index_in_l1_batch: Some(log.tx_index_in_l1_batch.into()),
            key: H256::from_slice(&log.key),
            value: H256::from_slice(&log.value),
        }
    }
}

use zksync_types::{
    api,
    l2_to_l1_log::{self, UserL2ToL1Log},
    web3::{Bytes, Index},
    Address, H256, U256, U64,
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
    pub block_timestamp: Option<i64>,
}

impl From<StorageWeb3Log> for api::Log {
    fn from(log: StorageWeb3Log) -> api::Log {
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
        api::Log {
            address: Address::from_slice(&log.address),
            topics,
            data: Bytes(log.value),
            block_hash: log.block_hash.map(|hash| H256::from_slice(&hash)),
            block_number: Some(U64::from(log.miniblock_number as u32)),
            l1_batch_number: log.l1_batch_number.map(U64::from),
            transaction_hash: Some(H256::from_slice(&log.tx_hash)),
            transaction_index: Some(Index::from(log.tx_index_in_block as u32)),
            log_index: Some(U256::from(log.event_index_in_block as u32)),
            transaction_log_index: Some(U256::from(log.event_index_in_tx as u32)),
            log_type: None,
            removed: Some(false),
            block_timestamp: log.block_timestamp.map(|t| (t as u64).into()),
        }
    }
}

#[derive(sqlx::FromRow, Debug, Clone)]
pub struct StorageL2ToL1Log {
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

impl From<StorageL2ToL1Log> for api::L2ToL1Log {
    fn from(log: StorageL2ToL1Log) -> api::L2ToL1Log {
        api::L2ToL1Log {
            block_hash: None,
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

impl From<StorageL2ToL1Log> for l2_to_l1_log::L2ToL1Log {
    fn from(log: StorageL2ToL1Log) -> l2_to_l1_log::L2ToL1Log {
        l2_to_l1_log::L2ToL1Log {
            shard_id: (log.shard_id as u32).try_into().unwrap(),
            is_service: log.is_service,
            tx_number_in_block: (log.tx_index_in_l1_batch as u32).try_into().unwrap(),
            sender: Address::from_slice(&log.sender),
            key: H256::from_slice(&log.key),
            value: H256::from_slice(&log.value),
        }
    }
}

impl From<StorageL2ToL1Log> for l2_to_l1_log::UserL2ToL1Log {
    fn from(log: StorageL2ToL1Log) -> l2_to_l1_log::UserL2ToL1Log {
        UserL2ToL1Log(log.into())
    }
}

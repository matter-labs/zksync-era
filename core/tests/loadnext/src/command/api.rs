use rand::RngCore;
use zksync_types::api;

use crate::{
    account_pool::SyncWallet,
    all::AllWeighted,
    rng::{LoadtestRng, WeightedRandom},
    sdk::EthNamespaceClient,
};

/// Helper enum for generating random block number.
#[derive(Debug, Copy, Clone)]
enum BlockNumber {
    Committed,
    Number,
}

impl AllWeighted for BlockNumber {
    fn all_weighted() -> &'static [(Self, f32)] {
        const DEFAULT_WEIGHT: f32 = 1.0;

        &[
            (Self::Committed, DEFAULT_WEIGHT),
            (Self::Number, DEFAULT_WEIGHT),
        ]
    }
}

#[derive(Debug, Copy, Clone)]
pub enum ApiRequestType {
    /// Requests block with full transactions list.
    BlockWithTxs,
    /// Requests account balance.
    Balance,
    /// Requests account-deployed contract events.
    GetLogs,
}

impl AllWeighted for ApiRequestType {
    fn all_weighted() -> &'static [(Self, f32)] {
        const DEFAULT_WEIGHT: f32 = 1.0;

        &[
            (Self::BlockWithTxs, DEFAULT_WEIGHT),
            (Self::Balance, DEFAULT_WEIGHT),
            (Self::GetLogs, DEFAULT_WEIGHT),
        ]
    }
}

#[derive(Debug, Copy, Clone)]
pub struct ApiRequest {
    /// Type of the request to be performed.
    pub request_type: ApiRequestType,
    /// ZKsync block number, generated randomly.
    pub block_number: api::BlockNumber,
}

impl ApiRequest {
    pub async fn random(wallet: &SyncWallet, rng: &mut LoadtestRng) -> Self {
        let block_number = random_block_number(wallet, rng).await;
        let request_type = ApiRequestType::random(rng);
        Self {
            request_type,
            block_number,
        }
    }
}

async fn random_block_number(wallet: &SyncWallet, rng: &mut LoadtestRng) -> api::BlockNumber {
    let block_number = BlockNumber::random(rng);
    match block_number {
        BlockNumber::Committed => api::BlockNumber::Committed,
        BlockNumber::Number => {
            // Choose a random block in the range `[0, latest_committed_block_number)`.
            match wallet
                .provider
                .get_block_by_number(api::BlockNumber::Committed, false)
                .await
            {
                Ok(Some(block_number)) => {
                    let block_number = block_number.number.as_u64();
                    let number = rng.next_u64() % block_number;
                    api::BlockNumber::Number(number.into())
                }
                _ => {
                    // Fallback to the latest committed block.
                    api::BlockNumber::Committed
                }
            }
        }
    }
}

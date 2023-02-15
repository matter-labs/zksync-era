use crate::all::AllWeighted;
use crate::config::ExplorerApiRequestWeights;
use crate::rng::{LoadtestRng, WeightedRandom};
use once_cell::sync::OnceCell;

static WEIGHTS: OnceCell<[(ExplorerApiRequestType, f32); 9]> = OnceCell::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExplorerApiRequestType {
    NetworkStats,
    Blocks,
    Block,
    Transaction,
    Transactions,
    AccountTransactions,
    Account,
    Contract,
    Token,
}

impl ExplorerApiRequestType {
    pub fn initialize_weights(weights: &ExplorerApiRequestWeights) {
        WEIGHTS
            .set([
                (ExplorerApiRequestType::NetworkStats, weights.network_stats),
                (ExplorerApiRequestType::Blocks, weights.blocks),
                (ExplorerApiRequestType::Block, weights.block),
                (ExplorerApiRequestType::Transaction, weights.transaction),
                (ExplorerApiRequestType::Transactions, weights.transactions),
                (
                    ExplorerApiRequestType::AccountTransactions,
                    weights.account_transactions,
                ),
                (ExplorerApiRequestType::Account, weights.account),
                (ExplorerApiRequestType::Contract, weights.contract),
                (ExplorerApiRequestType::Token, weights.token),
            ])
            .unwrap();
    }
}
impl AllWeighted for ExplorerApiRequestType {
    fn all_weighted() -> &'static [(Self, f32)] {
        WEIGHTS.get().expect("Weights are not initialized")
    }
}

#[derive(Debug, Copy, Clone)]
pub struct ExplorerApiRequest {
    /// Type of the request to be performed.
    pub request_type: ExplorerApiRequestType,
}

impl ExplorerApiRequest {
    pub async fn random(rng: &mut LoadtestRng) -> Self {
        let request_type = ExplorerApiRequestType::random(rng);
        Self { request_type }
    }
}

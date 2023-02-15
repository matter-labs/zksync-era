use crate::all::AllWeighted;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum SubscriptionType {
    /// Subscribes for new block headers.
    BlockHeaders,
    /// Subscribes for new transactions.
    PendingTransactions,
    /// Subscribes for new logs.
    Logs,
}

impl AllWeighted for SubscriptionType {
    fn all_weighted() -> &'static [(Self, f32)] {
        const DEFAULT_WEIGHT: f32 = 1.0;
        &[
            (Self::BlockHeaders, DEFAULT_WEIGHT),
            (Self::PendingTransactions, DEFAULT_WEIGHT),
            (Self::Logs, DEFAULT_WEIGHT),
        ]
    }
}

impl SubscriptionType {
    pub fn rpc_name(&self) -> &'static str {
        match self {
            Self::BlockHeaders => "newHeads",
            Self::PendingTransactions => "newPendingTransactions",
            Self::Logs => "logs",
        }
    }
}

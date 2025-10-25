/// The default maximal L1 gas price which L1->L2 transactions will assume.
/// This number influences how much fees will the script pay for L1->L2 transactions.
pub const DEFAULT_MAX_L1_GAS_PRICE_FOR_PRIORITY_TXS: u64 = 50_000_000_000;

pub const PAUSE_DEPOSITS_TIME_WINDOW_END: u64 = 7 * 24 * 60 * 60; // 7 days

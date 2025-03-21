use std::{ops, time::Duration};

/// Normally, block is committed on Ethereum every 15 seconds; however there are no guarantees that our transaction
/// will be included in the next block right after sending.
pub const ETH_CONFIRMATION_TIMEOUT: Duration = Duration::from_secs(300);

/// Normally, block is committed on Ethereum every 10-15 seconds, so there is no need to poll the ETH node
/// any frequently than once in 10 seconds.
pub const ETH_POLLING_INTERVAL: Duration = Duration::from_secs(10);

/// Loadtest assumes that blocks on the server will be created relatively quickly (without timeouts set in hours).
///
/// Nonetheless we want to provide some buffer in case we'll spam the server with way too many transactions
/// and some tx will have to wait in the mempool for a while.
pub const COMMIT_TIMEOUT: Duration = Duration::from_secs(600);

/// We don't want to overload the server with too many requests.
///
/// Given the fact that blocks are expected to be created every couple of seconds,
/// chosen value seems to be adequate to provide the result in one or two calls at average.
pub const POLLING_INTERVAL: ops::Range<Duration> = Duration::from_secs(2)..Duration::from_secs(3);

pub const MAX_OUTSTANDING_NONCE: usize = 20;

/// Each account continuously sends API requests in addition to transactions. Such requests are considered failed
/// after this amount of time elapsed without any server response.
pub const API_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Max number of L1 transactions that can be sent by a single account over the duration of the test.
pub const MAX_L1_TRANSACTIONS: u64 = 10;

/// Minimum amount of funds that should be present on the master account.
/// Loadtest won't start if master account balance is less than this value.
pub const MIN_MASTER_ACCOUNT_BALANCE: u128 = 2u128.pow(17);

/// Minimum amount of funds that should be present on the paymaster account.
/// Loadtest will deposit funds to the paymaster account if its balance is less than this value.
pub const MIN_PAYMASTER_BALANCE: u128 = 10u128.pow(18) * 50;

/// If the paymaster balance is lower than MIN_PAYMASTER_BALANCE,
/// loadtest will deposit funds to the paymaster account so that its balance reaches this value
pub const TARGET_PAYMASTER_BALANCE: u128 = 10u128.pow(18) * 60;

/// Min allowance for estimating the price for the paymaster transaction.
///
/// It should be roughly equal (or maybe a bit higher) than the actual used tokens in the transaction for the most precise
/// estimations. Note, however that is must not be higher than the ERC20 balance of the account.
pub const MIN_ALLOWANCE_FOR_PAYMASTER_ESTIMATE: u128 = 10u128.pow(18);

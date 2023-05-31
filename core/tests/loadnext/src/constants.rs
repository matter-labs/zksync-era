use std::time::Duration;

/// Normally, block is committed on Ethereum every 15 seconds; however there are no guarantees that our transaction
/// will be included in the next block right after sending.
pub const ETH_CONFIRMATION_TIMEOUT: Duration = Duration::from_secs(300);

/// Normally, block is committed on Ethereum every 10-15 seconds, so there is no need to poll the ETH node
/// any frequently than once in 10 seconds.
pub const ETH_POLLING_INTERVAL: Duration = Duration::from_secs(10);

/// Loadtest assumes that blocks on the server will be created relatively quickly (without timeouts set in hours),
/// but nonetheless we want to provide some buffer in case we'll spam the server with way too many transactions
/// and some tx will have to wait in the mempool for a while.
pub const COMMIT_TIMEOUT: Duration = Duration::from_secs(600);
/// We don't want to overload the server with too many requests; given the fact that blocks are expected to be created
/// every couple of seconds, chosen value seems to be adequate to provide the result in one or two calls at average.
pub const POLLING_INTERVAL: Duration = Duration::from_secs(3);

pub const MAX_OUTSTANDING_NONCE: usize = 20;

/// Each account continuously sends API requests in addition to transactions. Such requests are considered failed
/// after this amount of time elapsed without any server response.
pub const API_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

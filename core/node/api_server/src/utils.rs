//! Utils specific to the API server.

use std::{
    cell::Cell,
    thread,
    time::{Duration, Instant},
};

use zksync_dal::{Connection, Core, DalError};
use zksync_web3_decl::error::Web3Error;

/// Opens a readonly transaction over the specified connection.
pub(crate) async fn open_readonly_transaction<'r>(
    conn: &'r mut Connection<'_, Core>,
) -> Result<Connection<'r, Core>, Web3Error> {
    let builder = conn.transaction_builder().map_err(DalError::generalize)?;
    Ok(builder
        .set_readonly()
        .build()
        .await
        .map_err(DalError::generalize)?)
}

/// Allows filtering events (e.g., for logging) so that they are reported no more frequently than with a configurable interval.
///
/// Current implementation uses thread-local vars in order to not rely on mutexes or other cross-thread primitives.
/// I.e., it only really works if the number of threads accessing it is limited (which is the case for the API server;
/// the number of worker threads is congruent to the CPU count).
#[derive(Debug)]
pub(super) struct ReportFilter {
    interval: Duration,
    last_timestamp: &'static thread::LocalKey<Cell<Option<Instant>>>,
}

impl ReportFilter {
    // Should only be used from the `report_filter!` macro.
    pub const fn new(
        interval: Duration,
        last_timestamp: &'static thread::LocalKey<Cell<Option<Instant>>>,
    ) -> Self {
        Self {
            interval,
            last_timestamp,
        }
    }

    /// Should be called sparingly, since it involves moderately heavy operations (getting current time).
    pub fn should_report(&self) -> bool {
        let timestamp = self.last_timestamp.get();
        let now = Instant::now();
        if timestamp.is_none_or(|ts| now - ts > self.interval) {
            self.last_timestamp.set(Some(now));
            true
        } else {
            false
        }
    }
}

/// Creates a new filter with the specified reporting interval *per thread*.
macro_rules! report_filter {
    ($interval:expr) => {{
        thread_local! {
            static LAST_TIMESTAMP: std::cell::Cell<Option<std::time::Instant>> = std::cell::Cell::new(None);
        }
        ReportFilter::new($interval, &LAST_TIMESTAMP)
    }};
}

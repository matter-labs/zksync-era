//! Utils specific to the API server.

use std::{
    cell::Cell,
    thread,
    time::{Duration, Instant},
};

use anyhow::Context;
use zksync_dal::{Connection, Core, DalError};
use zksync_multivm::circuit_sequencer_api_latest::boojum::ethereum_types::U256;
use zksync_web3_decl::error::Web3Error;

pub(crate) fn prepare_evm_bytecode(raw: &[u8]) -> anyhow::Result<Vec<u8>> {
    // EVM bytecodes are prefixed with a big-endian `U256` bytecode length.
    let bytecode_len_bytes = raw.get(..32).context("length < 32")?;
    let bytecode_len = U256::from_big_endian(bytecode_len_bytes);
    let bytecode_len: usize = bytecode_len
        .try_into()
        .map_err(|_| anyhow::anyhow!("length ({bytecode_len}) overflow"))?;
    let bytecode = raw.get(32..(32 + bytecode_len)).with_context(|| {
        format!(
            "prefixed length ({bytecode_len}) exceeds real length ({})",
            raw.len() - 32
        )
    })?;
    // Since slicing above succeeded, this one is safe.
    let padding = &raw[(32 + bytecode_len)..];
    anyhow::ensure!(
        padding.iter().all(|&b| b == 0),
        "bytecode padding contains non-zero bytes"
    );
    Ok(bytecode.to_vec())
}

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
        if timestamp.map_or(true, |ts| now - ts > self.interval) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testonly::{PROCESSED_EVM_BYTECODE, RAW_EVM_BYTECODE};

    #[test]
    fn preparing_evm_bytecode() {
        let prepared = prepare_evm_bytecode(RAW_EVM_BYTECODE).unwrap();
        assert_eq!(prepared, PROCESSED_EVM_BYTECODE);
    }
}

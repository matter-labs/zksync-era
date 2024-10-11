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
    use const_decoder::Decoder;

    use super::*;

    #[test]
    fn preparing_evm_bytecode() {
        const RAW_BYTECODE: &[u8] = &const_decoder::decode!(
            Decoder::Hex,
            b"00000000000000000000000000000000000000000000000000000000000001266080604052348015\
              600e575f80fd5b50600436106030575f3560e01c8063816898ff146034578063fb5343f314604c57\
              5b5f80fd5b604a60048036038101906046919060a6565b6066565b005b6052606f565b604051605d\
              919060d9565b60405180910390f35b805f8190555050565b5f5481565b5f80fd5b5f819050919050\
              565b6088816078565b81146091575f80fd5b50565b5f8135905060a0816081565b92915050565b5f\
              6020828403121560b85760b76074565b5b5f60c3848285016094565b91505092915050565b60d381\
              6078565b82525050565b5f60208201905060ea5f83018460cc565b9291505056fea2646970667358\
              221220caca1247066da378f2ec77c310f2ae51576272367b4fa11cc4350af4e9ce4d0964736f6c63\
              4300081a00330000000000000000000000000000000000000000000000000000"
        );
        const EXPECTED_BYTECODE: &[u8] = &const_decoder::decode!(
            Decoder::Hex,
            b"6080604052348015600e575f80fd5b50600436106030575f3560e01c8063816898ff146034578063\
              fb5343f314604c575b5f80fd5b604a60048036038101906046919060a6565b6066565b005b605260\
              6f565b604051605d919060d9565b60405180910390f35b805f8190555050565b5f5481565b5f80fd\
              5b5f819050919050565b6088816078565b81146091575f80fd5b50565b5f8135905060a081608156\
              5b92915050565b5f6020828403121560b85760b76074565b5b5f60c3848285016094565b91505092\
              915050565b60d3816078565b82525050565b5f60208201905060ea5f83018460cc565b9291505056\
              fea2646970667358221220caca1247066da378f2ec77c310f2ae51576272367b4fa11cc4350af4e9\
              ce4d0964736f6c634300081a0033"
        );

        let prepared = prepare_evm_bytecode(RAW_BYTECODE).unwrap();
        assert_eq!(prepared, EXPECTED_BYTECODE);
    }
}

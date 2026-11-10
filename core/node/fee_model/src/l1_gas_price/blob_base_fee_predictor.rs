use zksync_dal::{Connection, Core, CoreDal};
use zksync_types::L1BatchNumber;

/// This function predicts the blob base fee for next bathes
/// based on the last known blob base fee and the number of blobs to send in the next batches.
///
/// `l1_block_number` is the L1 head that the gas adjuster fetched in the same poll, so this
/// function doesn't query L1 itself.
///
/// Returns an error if `l1_block_number` is behind the L1 block at which the last committed batch
/// was sent. That can happen if the gas adjuster's L1 provider lags behind the one used by
/// `eth_sender`. Treating the elapsed time as a single L1 block would put all pending blobs into
/// that block and push the prediction up to `max_blob_base_fee`.
///
/// WARNING: This function should be only used for L1, not for gateway settlement layer.
pub(crate) async fn predict_blob_base_fee(
    connection: &mut Connection<'_, Core>,
    l1_block_number: u32,
    last_known_l1_blob_fee: u64,
) -> anyhow::Result<u64> {
    let last_sealed_batch = connection
        .blocks_dal()
        .get_sealed_l1_batch_number()
        .await?
        .unwrap_or(L1BatchNumber::from(0));

    let (last_l1_commited_batch, last_commited_block_number) = connection
        .eth_sender_dal()
        .get_number_and_sent_at_block_for_latest_commited_batch(l1_block_number)
        .await?;

    let total_blobs_to_send = connection
        .blocks_dal()
        .get_blobs_amount_for_range(L1BatchNumber(last_l1_commited_batch + 1), last_sealed_batch)
        .await?;

    if total_blobs_to_send == 0 {
        return Ok(last_known_l1_blob_fee);
    }

    anyhow::ensure!(
        last_commited_block_number <= l1_block_number,
        "L1 head {l1_block_number} is behind L1 block {last_commited_block_number}, \
         at which the commit tx for the last committed L1 batch #{last_l1_commited_batch} was sent"
    );
    let total_l1_blocks_for_these_blocks =
        u64::from(l1_block_number - last_commited_block_number).max(1);

    tracing::debug!("Predicting blob fee cap with params: blobs_total: {total_blobs_to_send}, l1_blocks_total: {total_l1_blocks_for_these_blocks}, last_known_l1_gas_price: {last_known_l1_blob_fee}");

    Ok(predict_blob_fee_cap(
        total_blobs_to_send,
        total_l1_blocks_for_these_blocks,
        last_known_l1_blob_fee,
    ))
}

const MIN_BASE_FEE_PER_BLOB_GAS: u64 = 1;
const BLOB_BASE_FEE_UPDATE_FRACTION: u64 = 3_338_477;
const BLOB_GAS_PER_BLOB: u64 = 131_072;
const BLOB_GAS_PER_BLOCK_TARGET: u64 = 6 * BLOB_GAS_PER_BLOB; // target number of blobs per block(Ethereum uses 6)
const SAFETY_BPS: u32 = 11000; // +10%

fn predict_blob_fee_cap(blobs_total: u64, l1_blocks_total: u64, l1_blob_base_fee: u64) -> u64 {
    // excess = excess_from_fee(l1_blob_base_fee) + (blobs_total * BLOB_GAS_PER_BLOB - BLOB_GAS_PER_BLOCK_TARGET * l1_blocks_total)
    let excess = excess_from_fee(l1_blob_base_fee).saturating_add(
        BLOB_GAS_PER_BLOB
            .saturating_mul(blobs_total)
            .saturating_sub(BLOB_GAS_PER_BLOCK_TARGET.saturating_mul(l1_blocks_total)),
    );

    // --- return a safe cap = fee * (1 + safety)---
    ((base_fee_from_excess(excess) as u128) * (SAFETY_BPS as u128) / 10_000u128) as u64
}

fn fake_exponential(factor: u64, numerator: u64, denominator: u64) -> u64 {
    let mut i: u128 = 1;
    let mut output: u128 = 0;
    let mut accum: u128 = (factor as u128).saturating_mul(denominator as u128);
    let num = numerator as u128;
    let den = denominator as u128;
    while accum > 0 {
        output += accum;
        // accum = accum * num / (den * i)
        accum = (accum.saturating_mul(num)) / (den.saturating_mul(i));
        i += 1;
    }
    (output.saturating_div(den)) as u64
}

fn base_fee_from_excess(excess: u64) -> u64 {
    fake_exponential(
        MIN_BASE_FEE_PER_BLOB_GAS,
        excess,
        BLOB_BASE_FEE_UPDATE_FRACTION,
    )
}

fn excess_from_fee(target_fee: u64) -> u64 {
    if target_fee <= MIN_BASE_FEE_PER_BLOB_GAS {
        return 0;
    }
    // binary search
    let mut hi: u64 = 1;
    while base_fee_from_excess(hi) < target_fee && hi < u64::MAX / 2 {
        hi *= 2;
    }
    let mut lo: u64 = 0;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let f = base_fee_from_excess(mid);
        if f < target_fee {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_over_capacity_blobs() {
        // Capacity = 6 blobs per block * 10 blocks = 60 blobs
        // We give 100 blobs → backlog > 0
        let blobs_total = 100;
        let l1_blocks_total = 10;
        let l1_blob_base_fee = 1; // assume base fee at minimum

        let cap = predict_blob_fee_cap(blobs_total, l1_blocks_total, l1_blob_base_fee);
        assert_eq!(cap, 4, "fee cap should be 4");
    }

    #[test]
    fn test_normal_blobs() {
        // Exactly fits target capacity = 6 blobs * 5 blocks = 30 blobs
        let blobs_total = 30;
        let l1_blocks_total = 5;
        let l1_blob_base_fee = 1;

        let cap = predict_blob_fee_cap(blobs_total, l1_blocks_total, l1_blob_base_fee);
        // Expect only safety margin applied to base fee = 1
        let expected = ((SAFETY_BPS as u128) / 10_000u128) as u64;
        assert_eq!(
            cap, expected,
            "fee cap should equal base fee * safety when no backlog"
        );
    }

    #[test]
    fn test_extreme_backlog() {
        // 100 blobs per block * 10 blocks = 1000 blobs
        let blobs_total = 1000;
        let l1_blocks_total = 10;
        let l1_blob_base_fee = 1;

        let cap = predict_blob_fee_cap(blobs_total, l1_blocks_total, l1_blob_base_fee);
        assert_eq!(
            cap, 11_726_944_357_681_419,
            "extreme backlog should yield very high fee cap"
        );
    }

    #[test]
    fn test_a_lot_of_blobs_1_block() {
        // in this case, fee is not going to be too high, because exponential function will be not grow too fast
        let blobs_total = 100;
        let l1_blocks_total = 1;
        let l1_blob_base_fee = 1;

        let cap = predict_blob_fee_cap(blobs_total, l1_blocks_total, l1_blob_base_fee);
        assert_eq!(cap, 44, "fee cap should be 44");
    }
}

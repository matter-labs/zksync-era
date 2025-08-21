use zksync_dal::{Connection, Core, CoreDal};
use zksync_types::{
    aggregated_operations::{AggregatedActionType, L1BatchAggregatedActionType},
    web3::{BlockId, BlockNumber},
    L1BatchNumber,
};

use crate::l1_gas_price::GasAdjusterClient;

pub(crate) async fn predict_blob_base_fee(
    connection: &mut Connection<'_, Core>,
    client: &GasAdjusterClient,
    last_known_l1_gas_price: u64,
) -> u64 {
    let batch_stats = connection
        .eth_sender_dal()
        .get_eth_all_blocks_stat()
        .await
        .expect("Failed to get batch stats");
    let mut last_l1_commited_batch: Option<L1BatchNumber> = None;

    for (tx_type, block) in batch_stats.mined {
        if tx_type == AggregatedActionType::L1Batch(L1BatchAggregatedActionType::Commit) {
            last_l1_commited_batch = Some(block.into());
        }
    }

    let last_l1_commited_batch = last_l1_commited_batch.unwrap_or(L1BatchNumber::from(0));
    let last_sealed_batch = connection
        .blocks_dal()
        .get_sealed_l1_batch_number()
        .await
        .expect("Failed to get last sealed batch")
        .unwrap_or(L1BatchNumber::from(0));

    let total_blobs_to_send = connection
        .blocks_dal()
        .get_blobs_so_far(last_sealed_batch)
        .await
        .expect("Failed to get blobs so far for last sealed batch")
        - connection
            .blocks_dal()
            .get_blobs_so_far(last_l1_commited_batch)
            .await
            .expect("Failed to get blobs so far for last committed batch");

    let latest_block_number = client
        .inner
        .block(BlockId::Number(BlockNumber::Latest))
        .await
        .expect("Failed to get latest block")
        .expect("Latest block is None")
        .number
        .expect("Latest block number is None");
    let last_commited_block_number = connection
        .eth_sender_dal()
        .get_sent_at_block_for_commited_block(last_l1_commited_batch)
        .await
        .expect("Failed to get sent at block for commited block")
        .unwrap_or(latest_block_number.as_u32());

    let mut total_l1_blocks_for_these_blocks = latest_block_number
        .saturating_sub(last_commited_block_number.into())
        .as_u64();
    if total_l1_blocks_for_these_blocks == 0 {
        total_l1_blocks_for_these_blocks = 1;
    }

    if total_blobs_to_send == 0 {
        return last_known_l1_gas_price;
    }

    tracing::debug!("Predicting blob fee cap with params: blobs_total: {total_blobs_to_send}, l1_blocks_total: {total_l1_blocks_for_these_blocks}, last_known_l1_gas_price: {last_known_l1_gas_price}");

    predict_blob_fee_cap(
        total_blobs_to_send,
        total_l1_blocks_for_these_blocks,
        last_known_l1_gas_price,
    )
}

// todo: revisit this
const MIN_BASE_FEE_PER_BLOB_GAS: u64 = 1;
const BLOB_BASE_FEE_UPDATE_FRACTION: u64 = 3_338_477;
const BLOB_GAS_PER_BLOB: u64 = 131_072; // spec value
const BLOB_GAS_PER_BLOCK_TARGET: u64 = 393_216; // target number of blobs per block(Ethereum uses 3)
const CAPACITY_BLOBS_PER_BLOCK: u64 = 6; // spec max

// todo: revisit this
const SAFETY_BPS: u32 = 11000; // +10%
const TIP: u64 = 0; // wei per blob gas

fn predict_blob_fee_cap(
    blobs_total: u64,
    l1_blocks_total: u64,
    last_known_l1_gas_price: u64,
) -> u64 {
    // --- invert median_fee -> excess_now ---
    let mut excess = excess_from_fee(last_known_l1_gas_price);
    let mut max_fee = last_known_l1_gas_price;

    // --- planned path: spread blobs_total across blocks_total evenly ---
    let q = blobs_total / l1_blocks_total;
    let r = blobs_total % l1_blocks_total;
    for i in 0..l1_blocks_total {
        let served = q + if i < r { 1 } else { 0 };
        excess = next_excess(excess, served.min(CAPACITY_BLOBS_PER_BLOCK));
        let fee = base_fee_from_excess(excess);
        if fee > max_fee {
            max_fee = fee;
        }
    }

    // --- return a safe cap = max_fee * (1 + safety) + tip ---
    ((max_fee as u128) * (SAFETY_BPS as u128) / 10_000u128) as u64 + TIP
}

fn fake_exponential(factor: u64, numerator: u64, denominator: u64) -> u64 {
    let mut i: u128 = 1;
    let mut output: u128 = 0;
    let mut accum: u128 = (factor as u128) * (denominator as u128);
    let num = numerator as u128;
    let den = denominator as u128;
    while accum > 0 {
        output += accum;
        accum = (accum * num) / (den * i);
        i += 1;
    }
    (output / den) as u64
}

fn base_fee_from_excess(excess: u64) -> u64 {
    MIN_BASE_FEE_PER_BLOB_GAS.max(fake_exponential(
        BLOB_BASE_FEE_UPDATE_FRACTION,
        excess,
        BLOB_GAS_PER_BLOCK_TARGET,
    ))
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

fn next_excess(excess: u64, blobs_served: u64) -> u64 {
    excess
        .saturating_add(blobs_served.saturating_mul(BLOB_GAS_PER_BLOB))
        .saturating_sub(BLOB_GAS_PER_BLOCK_TARGET)
}

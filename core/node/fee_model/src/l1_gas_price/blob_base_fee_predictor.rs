use zksync_dal::{Connection, Core, CoreDal};
use zksync_types::{
    aggregated_operations::{AggregatedActionType, L1BatchAggregatedActionType},
    web3::{BlockId, BlockNumber},
    L1BatchNumber, U256,
};

use crate::l1_gas_price::GasAdjusterClient;

pub(crate) async fn predict_blob_base_fee(
    connection: &mut Connection<'_, Core>,
    client: &GasAdjusterClient,
    last_known_l1_gas_price: u64,
) -> u64 {
    let last_finalized_block = client
        .inner
        .block(BlockId::Number(BlockNumber::Finalized))
        .await
        .unwrap()
        .unwrap();

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

    let last_l1_commited_batch = last_l1_commited_batch.expect("No committed batch found");

    // todo: implement this
    let total_blobs_to_send = 0;

    let total_l1_blocks_for_these_blocks =
        U256::from(chrono::Utc::now().timestamp()) - last_finalized_block.timestamp;

    predict_blob_fee_cap(
        total_blobs_to_send,
        total_l1_blocks_for_these_blocks.as_u64(),
        last_known_l1_gas_price,
    )
}

// todo: revisit this
const MIN_BASE_FEE_PER_BLOB_GAS: u64 = 1;
const BLOB_BASE_FEE_UPDATE_FRACTION: u64 = 3_338_477;
const BLOB_GAS_PER_BLOB: u64 = 131_072; // spec value
const BLOB_GAS_PER_BLOCK_TARGET: u64 = 393_216; // e.g. 3 blobs * 131_072
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

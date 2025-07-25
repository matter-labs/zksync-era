use std::{sync::Arc, thread::sleep, time::Duration};

use anyhow::Context;
use chrono::{DateTime, Local};
use clap::Parser;
use cliclack::clear_screen;
use ethers::{
    providers::{Http, Middleware, Provider},
    types::{Filter, H256},
};
use zkstack_cli_common::{
    ethereum::{get_ethers_provider, get_zk_client_from_url},
    logger,
};
use zksync_basic_types::Address;
use zksync_contracts::hyperchain_contract;
use zksync_types::l1::L1Tx;
use zksync_web3_decl::{
    client::{Client, L2},
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
};

use crate::{consts::DEFAULT_EVENTS_BLOCK_RANGE, utils::addresses::apply_l1_to_l2_alias};

const DEFAULT_DISPLAYED_TXS_LIMIT: usize = 20;
const DEFAULT_INTERVALS_SECS: u64 = 2;

#[derive(Clone)]
enum PriorityTxState {
    Success,
    Fail,
    WaitingToProcess(u64),
}

#[derive(Clone)]
struct TxStatus {
    block_timestamp: u64,
    eth_tx_hash: H256,
    priority_tx_hash: H256,
    state: PriorityTxState,
}

#[derive(Clone, Debug, Parser)]
pub struct TrackPriorityOpsArgs {
    #[clap(long)]
    l1_rpc_url: String,
    #[clap(long)]
    l2_rpc_url: String,
    #[clap(long)]
    l1_op_sender: Option<Address>,
    #[clap(long)]
    from_block: Option<u64>,
    #[clap(long)]
    limit: Option<usize>,
    #[clap(long)]
    watch: Option<bool>,
    #[clap(long)]
    update_frequency_ms: Option<u64>,
    #[clap(long)]
    l2_tx_hash: Option<String>,
    #[clap(long)]
    l1_tx_hash: Option<String>,
}

fn ethers_log_to_zk_log(log: ethers::types::Log) -> zksync_types::web3::Log {
    zksync_types::web3::Log {
        address: log.address,
        topics: log.topics,
        data: zksync_types::web3::Bytes(log.data.into_iter().collect()),
        block_hash: log.block_hash,
        block_number: log.block_number,
        transaction_hash: log.transaction_hash,
        transaction_index: log.transaction_index.map(|idx| idx.as_u64().into()),
        log_index: log.log_index,
        transaction_log_index: log.transaction_log_index,
        log_type: log.log_type,
        removed: log.removed,
        block_timestamp: None, // ethers::types::Log does not contain timestamp
    }
}

/// Returns txs status.
/// Note, that for better UX it returns it in reversed order.
#[allow(clippy::too_many_arguments)]
async fn get_txs_status(
    l1_provider: Arc<Provider<Http>>,
    l2_zk_client: &Client<L2>,
    from_block: u64,
    limit: usize,
    diamond_proxy_address: Address,
    priority_op_sender: Option<Address>,
    l1_tx_hash: Option<H256>,
    l2_tx_hash: Option<H256>,
) -> anyhow::Result<Vec<TxStatus>> {
    let topic = hyperchain_contract()
        .event("NewPriorityRequest")
        .unwrap()
        .signature();

    // Get the latest block so we know how far we can go
    let latest_block = l1_provider
        .get_block_number()
        .await
        .expect("Failed to fetch latest block")
        .as_u64();

    // Instead of querying all logs at once, iterate over blocks in chunks to avoid throttling
    let mut priority_op_logs = Vec::new();
    let mut current_from = from_block;

    while current_from <= latest_block {
        let current_to = std::cmp::min(current_from + DEFAULT_EVENTS_BLOCK_RANGE - 1, latest_block);

        let filter = Filter::new()
            .address(diamond_proxy_address)
            .topic0(topic)
            .from_block(current_from)
            .to_block(current_to);

        let logs = l1_provider.get_logs(&filter).await?;
        priority_op_logs.extend(logs);

        current_from = current_to + 1;
    }

    let mut result = vec![];

    for log in priority_op_logs.into_iter().rev() {
        let log = ethers_log_to_zk_log(log);

        // All these expects are safe
        let eth_tx_hash = log
            .transaction_hash
            .context("Historical log lacking transaction hash")?;
        if let Some(expected_l1_tx_hash) = l1_tx_hash {
            if eth_tx_hash != expected_l1_tx_hash {
                continue;
            }
        }

        let block_number = log
            .block_number
            .context("Historical log lacking block number")?;
        let l1_tx = L1Tx::try_from(log).context("Failed to decode priority log")?;

        if let Some(expected_sender) = priority_op_sender {
            if l1_tx.common_data.sender != expected_sender {
                continue;
            }
        }

        let priority_tx_hash = l1_tx.hash();

        if let Some(expected_l2_tx_hash) = l2_tx_hash {
            if expected_l2_tx_hash != priority_tx_hash {
                continue;
            }
        }

        // To prevent rate limits on common providers, we need to `sleep` for a bit between frequent sequential calls
        sleep(Duration::from_millis(200));
        // Apparently, the block timestamp field is often not present, so we'll have to query it manually here
        let block_timestamp = l1_provider
            .get_block(block_number)
            .await?
            .context("Can not find block for Historical log")?
            .timestamp
            .as_u64();

        let tx_status = l2_zk_client
            .get_transaction_receipt(priority_tx_hash)
            .await?;

        match tx_status {
            Some(receipt) => result.push(TxStatus {
                block_timestamp,
                eth_tx_hash,
                priority_tx_hash,
                state: if receipt.status.as_u64() == 0 {
                    PriorityTxState::Fail
                } else {
                    PriorityTxState::Success
                },
            }),
            None => result.push(TxStatus {
                block_timestamp,
                eth_tx_hash,
                priority_tx_hash,
                state: PriorityTxState::WaitingToProcess(latest_block - block_number.as_u64() + 1),
            }),
        }

        if result.len() >= limit {
            return Ok(result);
        }
    }

    Ok(result)
}

fn display_statuses(statuses: Vec<TxStatus>) {
    if statuses.is_empty() {
        println!("No priority transactions were found with the given constraints");
    }

    for status in statuses.iter() {
        let (emoji, description) = match &status.state {
            PriorityTxState::Success => ("✅", "Success".to_string()),
            PriorityTxState::Fail => ("❌", "Failed".to_string()),
            PriorityTxState::WaitingToProcess(conf) => {
                ("⏳", format!("Waiting ({} confirmations)", conf))
            }
        };

        // Note, that here we use `println` instead of native logging to better control
        // the output format.
        println!("{} {} {:#?}", emoji, description, status.priority_tx_hash);
        println!("\tEthereum transaction hash: {:?}", status.eth_tx_hash);
        println!(
            "\tEthereum block timestamp: {:?}",
            pretty_print_timestamp(status.block_timestamp)
        );
    }
}

// Main function
pub async fn run(args: TrackPriorityOpsArgs) -> anyhow::Result<()> {
    let l1_provider = get_ethers_provider(&args.l1_rpc_url)?;
    let l2_zk_client = get_zk_client_from_url(&args.l2_rpc_url).await?;

    let from_block = if let Some(x) = args.from_block {
        x
    } else {
        let default_from_block = l1_provider
            .get_block_number()
            .await?
            .as_u64()
            .saturating_sub(DEFAULT_EVENTS_BLOCK_RANGE);
        logger::info(format!("No `--from-block` provided. {default_from_block} ({DEFAULT_EVENTS_BLOCK_RANGE} away from latest) will be used as a default", ));
        default_from_block
    };

    let limit = args.limit.unwrap_or_else(|| {
        logger::info(format!(
            "No `--limit` provided. {DEFAULT_DISPLAYED_TXS_LIMIT} will be used as default"
        ));
        DEFAULT_DISPLAYED_TXS_LIMIT
    });

    let delay = args.update_frequency_ms
        .map(Duration::from_millis)
        .unwrap_or_else(|| {
            logger::info(format!(
                "No `--upgrade-frequency-ms` provided. {DEFAULT_INTERVALS_SECS} seconds will be used as default"
            ));
            Duration::from_secs(DEFAULT_INTERVALS_SECS)
        });

    let priority_op_sender = if let Some(l1_sender) = args.l1_op_sender {
        // The actual sender may be aliased
        // We check whether it is the case by checking the code size

        let sender_code_size = l1_provider.get_code(l1_sender, None).await?.len();

        if sender_code_size > 0 {
            Some(apply_l1_to_l2_alias(l1_sender))
        } else {
            // Unchanged
            Some(l1_sender)
        }
    } else {
        None
    };

    let l1_tx_hash = args.l1_tx_hash.map(|x| x.parse()).transpose()?;
    let l2_tx_hash = args.l2_tx_hash.map(|x| x.parse()).transpose()?;

    let diamond_proxy_address = l2_zk_client.get_main_l1_contract().await?;

    if !args.watch.unwrap_or_default() {
        let statuses = get_txs_status(
            l1_provider.clone(),
            &l2_zk_client,
            from_block,
            limit,
            diamond_proxy_address,
            priority_op_sender,
            l1_tx_hash,
            l2_tx_hash,
        )
        .await?;

        display_statuses(statuses);
        return Ok(());
    }

    loop {
        let statuses = get_txs_status(
            l1_provider.clone(),
            &l2_zk_client,
            from_block,
            limit,
            diamond_proxy_address,
            priority_op_sender,
            l1_tx_hash,
            l2_tx_hash,
        )
        .await?;

        clear_screen()?; // clear before each redraw

        display_statuses(statuses);

        sleep(delay);
    }
}

fn pretty_print_timestamp(timestamp: u64) -> String {
    let datetime = DateTime::from_timestamp(timestamp as i64, 0).unwrap();
    let datetime: DateTime<Local> =
        DateTime::from_naive_utc_and_offset(datetime.naive_utc(), *Local::now().offset());
    datetime.format("%b-%d-%Y %I:%M:%S %p UTC%:z").to_string()
}

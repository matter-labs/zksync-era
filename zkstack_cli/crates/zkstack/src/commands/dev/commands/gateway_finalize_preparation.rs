use std::{
    collections::HashSet,
    fs::File,
    io::{BufReader, BufWriter},
    num::NonZeroUsize,
    path::Path,
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use clap::{Parser, ValueEnum};
// get_list_of_tokens.rs
use ethers::prelude::*;
use ethers::{
    abi::{encode, parse_abi, Token},
    contract::{abigen, BaseContract},
    providers::{Http, Middleware, Provider},
    utils::hex,
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use strum::EnumIter;
use tokio::time::sleep;
use xshell::Shell;
use zkstack_cli_config::{
    forge_interface::gateway_ecosystem_upgrade::output::GatewayEcosystemUpgradeOutput,
    traits::{ReadConfig, ZkStackConfig},
    ContractsConfig,
};
use zksync_contracts::{chain_admin_contract, hyperchain_contract, DIAMOND_CUT};
use zksync_types::{
    ethabi, h256_to_address, h256_to_u256, u256_to_h256,
    url::SensitiveUrl,
    web3::{keccak256, Bytes},
    Address, L1BatchNumber, L2BlockNumber, L2ChainId, ProtocolVersionId, H256,
    L2_NATIVE_TOKEN_VAULT_ADDRESS, SHARED_BRIDGE_ETHER_TOKEN_ADDRESS, U256,
};
use zksync_web3_decl::{
    client::{Client, DynClient, L2},
    namespaces::{UnstableNamespaceClient, ZksNamespaceClient},
};

use super::gateway::GatewayUpgradeInfo;

/// Structure that we expect to read from / write to the existing cache file
#[derive(Debug, Serialize, Deserialize)]
struct Cache {
    first_seen_block: u64,
    last_seen_block: u64,
    added_tokens: Vec<Address>,
}

/// How many blocks to process in one chunk
const BLOCK_RANGE: u64 = 50_000;

/// This is the modified function that also accepts a `legacy_bridge_addr`.
///
/// It will query:
///  - The `BridgehubDepositInitiated` event at `bridge_addr`.
///  - The `DepositInitiated(bytes32,address,address,address,uint256)` event at `legacy_bridge_addr`.
///  - The `DepositInitiated(address,address,address,uint256)` event at `legacy_bridge_addr`.
///
/// For each event, it parses out `l1Token` and stores it.
pub async fn get_list_of_tokens(
    block_to_start_with: u64,
    existing_cache_path: &str,
    l1_rpc_url: &str,
    // The new "bridge hub" address (already used).
    bridge_addr: Address,
    // The additional legacy bridge address.
    legacy_bridge_addr: Address,
    base_tokens: Vec<Address>,
) -> Vec<Address> {
    // ---------------------------------------------------------
    // 1. Read or initialize the cache
    // ---------------------------------------------------------
    let mut cache = read_cache_from_file(&existing_cache_path).unwrap_or_else(|| Cache {
        first_seen_block: block_to_start_with,
        last_seen_block: block_to_start_with,
        added_tokens: vec![],
    });

    // If the cache file was found, check the condition about `first_seen_block`
    if cache.first_seen_block > block_to_start_with {
        // If the cache's first_seen_block is larger than our new start,
        // clear the entire cache and reset.
        cache.first_seen_block = block_to_start_with;
        cache.last_seen_block = block_to_start_with;
        cache.added_tokens.clear();
    }

    // We'll use a HashSet for tracking tokens to avoid duplicates easily
    let mut discovered_tokens: HashSet<Address> = cache.added_tokens.iter().copied().collect();

    // Ensure that all base tokens are marked as discovered.
    for base_token in base_tokens {
        discovered_tokens.insert(base_token);
    }

    discovered_tokens.insert(SHARED_BRIDGE_ETHER_TOKEN_ADDRESS);

    // ---------------------------------------------------------
    // 2. Connect to a provider
    // ---------------------------------------------------------
    let provider =
        Provider::<Http>::try_from(l1_rpc_url).expect("Could not instantiate HTTP Provider");

    // Get the latest block so we know how far we can go
    let latest_block = provider
        .get_block_number()
        .await
        .expect("Failed to fetch latest block")
        .as_u64();

    // Our actual starting point is whichever is further along
    let mut current_block = cache.last_seen_block;

    // ---------------------------------------------------------
    // 3. Process logs in chunks of BLOCK_RANGE
    // ---------------------------------------------------------
    while current_block <= latest_block {
        let start_of_range = current_block;
        let end_of_range = std::cmp::min(start_of_range + BLOCK_RANGE, latest_block);

        println!("Processing range {start_of_range} - {end_of_range}\n");

        // If the entire range is below what we have already processed, skip
        if end_of_range < cache.last_seen_block {
            // skip range
            current_block = end_of_range + 1;
            println!("Range is cached, skipping...");
            continue;
        }

        // ---------------------------------------------------------
        // 4. Build filters and fetch logs, parse out the l1Token
        // ---------------------------------------------------------

        // a) BridgehubDepositInitiated event at `bridge_addr`
        let filter_bridgehub_deposit_initiated = Filter::new()
            .address(bridge_addr)
            .event("BridgehubDepositInitiated(uint256,bytes32,address,address,address,uint256)")
            .from_block(start_of_range)
            .to_block(end_of_range);

        // Sleep for 1 second before the JSON-RPC request
        sleep(Duration::from_secs(1)).await;
        let logs_bridgehub = provider
            .get_logs(&filter_bridgehub_deposit_initiated)
            .await
            .expect("Failed to fetch logs for BridgehubDepositInitiated");

        for log in logs_bridgehub {
            // The event is:
            // BridgehubDepositInitiated(uint256 chainId, bytes32 txDataHash, address from,
            //                           address to, address l1Token, uint256 amount)
            // If chainId, txDataHash, from are indexed, the next unindexed parameters in `log.data`
            // are `to`, `l1Token`, `amount` in that order.
            // So `l1Token` is in offset 32..64 within `log.data`.
            let raw_data = log.data.0;
            if raw_data.len() < 64 {
                // Malformed log data, skip
                continue;
            }
            let l1_token_bytes = &raw_data[32..64];
            let l1_token_addr = Address::from_slice(&l1_token_bytes[12..32]); // last 20 bytes
            discovered_tokens.insert(l1_token_addr);
        }

        // b) Legacy deposit with signature: DepositInitiated(bytes32,address,address,address,uint256)
        //    - l1Token is unindexed at the first 32 bytes in the log data.
        let filter_legacy_deposit_initiated_1 = Filter::new()
            .address(legacy_bridge_addr)
            .event("DepositInitiated(bytes32,address,address,address,uint256)")
            .from_block(start_of_range)
            .to_block(end_of_range);

        // Sleep for 1 second before the JSON-RPC request
        sleep(Duration::from_secs(1)).await;
        let logs_legacy_1 = provider
            .get_logs(&filter_legacy_deposit_initiated_1)
            .await
            .expect(
            "Failed to fetch logs for DepositInitiated - (bytes32,address,address,address,uint256)",
        );

        for log in logs_legacy_1 {
            // The event layout is:
            //   event DepositInitiated(
            //       bytes32 indexed l2DepositTxHash,
            //       address indexed from,
            //       address indexed to,
            //       address l1Token,
            //       uint256 amount
            //   );
            //
            // Indexed:   l2DepositTxHash, from, to  (=> topics[1..=3])
            // Unindexed: l1Token (32 bytes), amount (32 bytes) => log.data
            // So `log.data[0..32]` is l1Token, `log.data[32..64]` is amount.
            let raw_data = log.data.0;
            if raw_data.len() < 64 {
                continue;
            }
            // l1Token is the first 32 bytes
            let l1_token_bytes = &raw_data[0..32];
            let l1_token_addr = Address::from_slice(&l1_token_bytes[12..32]);
            discovered_tokens.insert(l1_token_addr);
        }

        // c) Legacy deposit with signature: DepositInitiated(address,address,address,uint256)
        //    - l1Token is fully indexed, so it's the 3rd indexed address in topics[3].
        let filter_legacy_deposit_initiated_2 = Filter::new()
            .address(legacy_bridge_addr)
            .event("DepositInitiated(address,address,address,uint256)")
            .from_block(start_of_range)
            .to_block(end_of_range);

        // Sleep for 1 second before the JSON-RPC request
        sleep(Duration::from_secs(1)).await;
        let logs_legacy_2 = provider
            .get_logs(&filter_legacy_deposit_initiated_2)
            .await
            .expect(
                "Failed to fetch logs for DepositInitiated - (address,address,address,uint256)",
            );

        for log in logs_legacy_2 {
            // The event layout is:
            //   event DepositInitiated(
            //       address indexed from,
            //       address indexed to,
            //       address indexed l1Token,
            //       uint256 amount
            //   );
            //
            // Indexed: from (topics[1]), to (topics[2]), l1Token (topics[3])
            // The data field only has `amount`.
            if log.topics.len() < 4 {
                continue;
            }
            let l1_token_topic = log.topics[3];
            // Parse last 20 bytes of the topic
            let l1_token_addr = Address::from_slice(&l1_token_topic.as_bytes()[12..32]);
            discovered_tokens.insert(l1_token_addr);
        }

        // ---------------------------------------------------------
        // 5. Update the cache, flush it to disk
        // ---------------------------------------------------------
        cache.last_seen_block = end_of_range;
        cache.added_tokens = discovered_tokens.iter().copied().collect();

        write_cache_to_file(&existing_cache_path, &cache).expect("Failed to write cache to file");

        println!("Processed and saved the range!");

        // Move our current_block pointer forward
        if end_of_range == latest_block {
            break;
        } else {
            current_block = end_of_range + 1;
        }
    }

    // Convert our HashSet to a Vec for the final result
    discovered_tokens.into_iter().collect()
}

/// Reads the cache JSON file if it exists and can be parsed.
fn read_cache_from_file<P: AsRef<Path>>(path: P) -> Option<Cache> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).ok()
}

/// Writes the updated cache to disk, overwriting the old file.
fn write_cache_to_file<P: AsRef<Path>>(
    path: P,
    cache: &Cache,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, &cache)?;
    Ok(())
}

abigen!(
    LegacyStateTransitionManagerAbi,
    r"[
    function getAllHyperchainChainIDs()(uint256[])
    function getHyperchain(uint256 _chainId) public view returns (address)
]"
);

abigen!(
    LegacyBridgehubAbi,
    r"[
    function chainTypeManager(uint256)(address)
    function baseToken(uint256)(address)
]"
);

async fn ask_storage(provider: Arc<Provider<Http>>, address: Address, key: H256) -> H256 {
    sleep(Duration::from_secs(1)).await;
    provider.get_storage_at(address, key, None).await.unwrap()
}

// The method has been deleted, so we'll have to replicate it in rust
async fn get_all_hyperchains_ids(provider: Arc<Provider<Http>>, stm_address: Address) -> Vec<U256> {
    let num_chains_key = u256_to_h256(U256::from(151));

    let number_of_chains =
        h256_to_u256(ask_storage(provider.clone(), stm_address, num_chains_key).await);

    // keccak(151)
    let initial_slot = h256_to_u256(
        H256::from_str("354a83ed9988f79f6038d4c7a7dadbad8af32f4ad6df893e0e5807a1b1944ff9").unwrap(),
    );

    let mut result = vec![];

    for i in 0..number_of_chains.as_u32() {
        let current_key = initial_slot + U256::from(i);

        let current_value =
            ask_storage(provider.clone(), stm_address, u256_to_h256(current_key)).await;

        result.push(h256_to_u256(current_value));
    }

    result
}

/// Returns an array of chains' ids and base tokens
async fn get_chains_info(
    l1_rpc_url: &str,
    bridgehub_addr: Address,
    era_chain_id: u64,
) -> (Vec<U256>, Vec<Address>) {
    let provider = Arc::new(
        Provider::<Http>::try_from(l1_rpc_url).expect("Could not instantiate HTTP Provider"),
    );

    let bridgehub = LegacyBridgehubAbi::new(bridgehub_addr, provider.clone());
    let stm_address = bridgehub
        .chain_type_manager(U256::from(era_chain_id))
        .call()
        .await
        .unwrap();

    if stm_address == Address::zero() {
        panic!("Era has not STM!");
    }

    let stm = LegacyStateTransitionManagerAbi::new(stm_address, provider.clone());

    let chain_ids = get_all_hyperchains_ids(provider.clone(), stm_address).await;

    let mut addresses = vec![];
    for chain in chain_ids.iter() {
        sleep(Duration::from_secs(1)).await;
        let chain = stm.get_hyperchain(*chain).call().await.unwrap();

        let base_token = ask_storage(provider.clone(), chain, u256_to_h256(U256::from(43))).await;

        addresses.push(h256_to_address(&base_token));
    }

    (chain_ids, addresses)
}

#[derive(Parser, Debug, Clone)]
pub struct GatewayFinalizePreparationArgs {
    upgrade_description_path: String,
    l1_rpc_url: String,
    block_to_start_with: u64,
    era_chain_id: u64,
    multicall_with_gas_addr: Address,
    #[clap(long, default_missing_value = "false")]
    send_finalize: bool,
}

const CACHE_PATH: &str = "tokens-cache.json";

// ZKChain ABI
abigen!(
    LegacySharedBridgeAbi,
    r"[
    function legacyBridge()(address)
]"
);

async fn get_legacy_bridge(bridge_addr: Address, l1_rpc_url: &str) -> Address {
    let provider =
        Provider::<Http>::try_from(l1_rpc_url).expect("Could not instantiate HTTP Provider");

    let bridge = LegacySharedBridgeAbi::new(bridge_addr, Arc::new(provider));

    bridge.legacy_bridge().call().await.unwrap()
}

lazy_static! {
    static ref FINALIZE_UPGRADE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function finalizeInit(address aggregator, address bridgehub,address payable l1NativeTokenVault,address[] calldata tokens,uint256[] calldata chains) external",
        ])
        .unwrap(),
    );
}

pub(crate) async fn run(shell: &Shell, args: GatewayFinalizePreparationArgs) -> anyhow::Result<()> {
    let upgrade_info = GatewayUpgradeInfo::read(shell, &args.upgrade_description_path)?;

    println!("Obtaining chain and base token info...");
    let (chain_ids, base_tokens) = get_chains_info(
        &args.l1_rpc_url,
        upgrade_info.bridgehub_addr,
        args.era_chain_id,
    )
    .await;

    println!("Obtaining bridged tokens info...");

    let tokens = get_list_of_tokens(
        args.block_to_start_with,
        CACHE_PATH,
        &args.l1_rpc_url,
        upgrade_info.l1_legacy_shared_bridge,
        get_legacy_bridge(upgrade_info.l1_legacy_shared_bridge, &args.l1_rpc_url).await,
        base_tokens,
    )
    .await;

    // Now, since we have the list of tokens and chains, we need to register those.

    let calldata = FINALIZE_UPGRADE
        .encode(
            "finalizeInit",
            (
                args.multicall_with_gas_addr,
                upgrade_info.bridgehub_addr,
                upgrade_info.native_token_vault_addr,
                tokens,
                chain_ids,
            ),
        )
        .unwrap();

    println!("data: {}", hex::encode(&calldata.0));

    Ok(())
}

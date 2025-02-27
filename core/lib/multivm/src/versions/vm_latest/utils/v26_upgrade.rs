use ethabi::{ParamType, Token};
use zksync_contracts::{l2_asset_router, l2_legacy_shared_bridge};
use zksync_types::{address_to_h256, get_address_mapping_key, get_immutable_simulator_key, h256_to_address, h256_to_u256, tx::execute::Create2DeploymentParams, utils::encode_ntv_asset_id, AccountTreeId, Address, StorageKey, Transaction, TransactionTimeRangeConstraint, H256, L2_ASSET_ROUTER_ADDRESS, L2_ASSET_ROUTER_LEGACY_SHARED_BRIDGE_IMMUTABLE_KEY, L2_ASSET_ROUTER_LEGACY_SHARED_BRIDGE_L1_CHAIN_ID_KEY, L2_LEGACY_SHARED_BRIDGE_BEACON_PROXY_BYTECODE_KEY, L2_LEGACY_SHARED_BRIDGE_L1_ADDRESSES_KEY, L2_LEGACY_SHARED_BRIDGE_UPGRADEABLE_BEACON_ADDRESS_KEY, L2_NATIVE_TOKEN_VAULT_ADDRESS, L2_NATIVE_TOKEN_VAULT_ASSET_ID_MAPPING_INDEX, U256};

/// A small trait that provides the interface to 
/// read a single storage key,
/// Unlike `ReadStorage` trait, it is async.
#[async_trait::async_trait]
pub trait AsyncStorageKeyAccess {
    async fn read_key(&mut self, key: &StorageKey) -> anyhow::Result<H256>;
}

fn extract_token_from_legacy_deposit(legacy_deposit_data: &[u8]) -> Option<Address> {
    let contract = l2_legacy_shared_bridge();
    let function = contract.function("finalizeDeposit").unwrap();

    let short_sig = function.short_signature();

    if legacy_deposit_data.len() < 4 || short_sig != legacy_deposit_data[..4] {
        return None;
    }

    let decoded = function.decode_input(&legacy_deposit_data[4..]).ok()?;
    decoded[2].clone().into_address()
}

fn extract_token_from_asset_router_deposit(
    l1_chain_id: U256,
    asset_router_deposit_data: &[u8],
) -> Option<Address> {
    let contract = l2_asset_router();

    // There are two types of `finalizeDeposit` functions:
    // - The one with 5 params that maintains the same interface as the one in the legacy contract
    // - The one with 3 params with the new interface.

    // TODO: maybe add a unit test to enforce that there are always two functions.
    let finalize_deposit_functions = contract.functions.get("finalizeDeposit")?;
    let finalize_deposit_3_params = finalize_deposit_functions
        .iter()
        .find(|f| f.inputs.len() == 3)
        .unwrap();
    let finalize_deposit_5_params = finalize_deposit_functions
        .iter()
        .find(|f| f.inputs.len() == 5)
        .unwrap();
    assert_eq!(
        finalize_deposit_functions.len(),
        2,
        "Unexpected ABI of L2AssetRouter"
    );

    if asset_router_deposit_data.len() < 4 {
        return None;
    }

    // Firstly, we test against the 5-input one as it is simpler.
    if finalize_deposit_5_params.short_signature() == asset_router_deposit_data[..4] {
        let decoded = finalize_deposit_5_params
            .decode_input(&asset_router_deposit_data[4..])
            .ok()?;
        return decoded[2].clone().into_address();
    }

    // Now, we test the option when the 3-paramed function is used.
    if finalize_deposit_3_params.short_signature() != asset_router_deposit_data[..4] {
        return None;
    }

    let decoded = finalize_deposit_3_params
        .decode_input(&asset_router_deposit_data[4..])
        .ok()?;
    let used_asset_id = H256::from_slice(&decoded[1].clone().into_fixed_bytes()?);
    let bridge_mint_data = decoded[2].clone().into_bytes()?;

    let decoded_ntv_input_data = ethabi::decode(
        &[
            ParamType::Address,
            ParamType::Address,
            ParamType::Address,
            ParamType::Uint(256),
            ParamType::Bytes,
        ],
        &bridge_mint_data,
    )
    .ok()?;
    let origin_token_address = decoded_ntv_input_data[2].clone().into_address()?;

    // We will also double check that the assetId corresponds to the assetId of a token that is bridged from l1
    let expected_asset_id = encode_ntv_asset_id(l1_chain_id, origin_token_address);

    // The used asset id is wrong, we should not rely on the token address to be processed by the native token vault
    if expected_asset_id != used_asset_id {
        return None;
    }

    Some(origin_token_address)
}


async fn calculate_expected_token_address(
    storage: &mut impl AsyncStorageKeyAccess,
    l2_legacy_shared_bridge_address: Address,
    l1_token_address: Address,
) -> anyhow::Result<Address> {
    println!("cc l2_legacy_shared_bridge_address = {:#?}", l2_legacy_shared_bridge_address);
    println!("cc l1_token_address = {:#?}", l1_token_address);

    // The source of truth for this logic is `L2SharedBridgeLegacy._calculateCreate2TokenAddress`

    let beacon_proxy_bytecode_hash_key = StorageKey::new(
        AccountTreeId::new(l2_legacy_shared_bridge_address),
        L2_LEGACY_SHARED_BRIDGE_BEACON_PROXY_BYTECODE_KEY,
    );
    let beacon_proxy_bytecode_hash = storage
        .read_key(&beacon_proxy_bytecode_hash_key)
        .await?;

    let beacon_address_key = StorageKey::new(
        AccountTreeId::new(l2_legacy_shared_bridge_address),
        L2_LEGACY_SHARED_BRIDGE_UPGRADEABLE_BEACON_ADDRESS_KEY,
    );
    let beacon_address = storage
        .read_key(&beacon_address_key)
        .await?;
    let beacon_address = h256_to_address(&beacon_address);

    let params = Create2DeploymentParams {
        salt: address_to_h256(&l1_token_address),
        bytecode_hash: beacon_proxy_bytecode_hash,
        raw_constructor_input: ethabi::encode(&[
            Token::Address(beacon_address),
            Token::Bytes(vec![]),
        ]),
    };

    Ok(params.derive_address(l2_legacy_shared_bridge_address))
}

/// Checks whether the token is legacy. It is legacy if both of the
/// following is true:
/// - It is present in legacy shared bridge
/// - It is not present in the L2 native token vault
async fn is_l2_token_legacy(
    storage: &mut impl AsyncStorageKeyAccess,
    l2_legacy_shared_bridge_address: Address,
    l2_token_address: Address,
    expected_l1_address: Address,
) -> anyhow::Result<bool> {
    // 1. Read l1 token address from L2 shared bridge (must not be 0)

    let stored_l1_address_key = StorageKey::new(
        AccountTreeId::new(l2_legacy_shared_bridge_address),
        get_address_mapping_key(&l2_token_address, L2_LEGACY_SHARED_BRIDGE_L1_ADDRESSES_KEY),
    );
    let stored_l1_address = storage
        .read_key(&stored_l1_address_key)
        .await?;
    let stored_l1_address = h256_to_address(&stored_l1_address);

    // No address is stored, it means that the token has never been bridged before
    // and thus, it is not legacy
    if stored_l1_address == Address::zero() {
        return Ok(false);
    }

    // Just for cross check
    assert_eq!(expected_l1_address, stored_l1_address);

    // 2. Read assetId from NTV (must be 0)
    let stored_asset_id_key = StorageKey::new(
        AccountTreeId::new(L2_NATIVE_TOKEN_VAULT_ADDRESS),
        get_address_mapping_key(
            &l2_token_address,
            L2_NATIVE_TOKEN_VAULT_ASSET_ID_MAPPING_INDEX,
        ),
    );
    let stored_asset_id = storage
        .read_key(&stored_asset_id_key)
        .await?;

    Ok(stored_asset_id == H256::zero())
}

/// Accepts a list of transactions to be included into the mempool and filters
/// the unsafe deposits and returns two vectors:
/// - Transactions to include into the mempool
/// - Transactions to return to the mempool
///
/// Note, that the purpose of this function is not to find unsafe deposits *only*
/// but detect whether they may be present at all. It does check that, e.g. the sender
/// of the transactions is the correct l1 bridge.
pub async fn is_unsafe_deposit_present(
    txs: &[(Transaction, TransactionTimeRangeConstraint)],
    storage: &mut impl AsyncStorageKeyAccess,
) -> anyhow::Result<bool> {
    // The rules are the following:
    // - All L2 transactions are allowed
    // - L1 transactions are allowed only if it is a deposit to an already registered token or
    // to a non-legacy token.

    // Firstly, let's check whether the chain has a legacy bridge.
    let legacy_bridge_key = get_immutable_simulator_key(
        &L2_ASSET_ROUTER_ADDRESS,
        L2_ASSET_ROUTER_LEGACY_SHARED_BRIDGE_IMMUTABLE_KEY,
    );

    let legacy_l2_shared_bridge_addr = storage
        .read_key(&legacy_bridge_key)
        .await?;
    let legacy_l2_shared_bridge_addr = h256_to_address(&legacy_l2_shared_bridge_addr);

    // There is either no legacy bridge or the L2AssetRouter has not been depoyed yet.
    // In both cases, there can be no unsafe deposits.
    if legacy_l2_shared_bridge_addr == Address::zero() {
        return Ok(false);
    }

    // In theory it could be fetched from config, but we do it here for consistency 
    let l1_chain_id_key = get_immutable_simulator_key(
        &L2_ASSET_ROUTER_ADDRESS,
        L2_ASSET_ROUTER_LEGACY_SHARED_BRIDGE_L1_CHAIN_ID_KEY,
    );
    let l1_chain_id = storage
        .read_key(&l1_chain_id_key)
        .await?;
    let l1_chain_id = h256_to_u256(l1_chain_id);

    for (tx, _) in txs {
        if !tx.is_l1() {
            // Not a deposit
            continue;
        }

        let Some(contract_address) = tx.execute.contract_address else {
            continue;
        };

        let l1_token_address = if contract_address == legacy_l2_shared_bridge_addr {
            extract_token_from_legacy_deposit(tx.execute.calldata())
        } else if contract_address == L2_ASSET_ROUTER_ADDRESS {
            // FIXME: chain id not 0
            extract_token_from_asset_router_deposit(l1_chain_id, tx.execute.calldata())
        } else {
            None
        };

        let Some(l1_token_address) = l1_token_address else {
            continue;
        };

        let l2_token_address = calculate_expected_token_address(
            storage,
            legacy_l2_shared_bridge_addr,
            l1_token_address,
        )
        .await?;

        if is_l2_token_legacy(
            storage,
            legacy_l2_shared_bridge_addr,
            l2_token_address,
            l1_token_address,
        )
        .await?
        {
            return Ok(true);
        }
    }

    Ok(false)
}

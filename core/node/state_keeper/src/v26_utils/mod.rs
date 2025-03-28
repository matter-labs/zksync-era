use zksync_contracts::{l2_asset_router, l2_legacy_shared_bridge};
use zksync_dal::{Connection, Core, CoreDal};
use zksync_types::{
    address_to_h256,
    ethabi::{self, ParamType, Token},
    get_address_mapping_key, get_immutable_simulator_key, h256_to_address, h256_to_u256,
    tx::execute::Create2DeploymentParams,
    utils::encode_ntv_asset_id,
    AccountTreeId, Address, L2BlockNumber, StorageKey, Transaction, H256, L2_ASSET_ROUTER_ADDRESS,
    L2_ASSET_ROUTER_LEGACY_SHARED_BRIDGE_IMMUTABLE_KEY,
    L2_ASSET_ROUTER_LEGACY_SHARED_BRIDGE_L1_CHAIN_ID_KEY,
    L2_LEGACY_SHARED_BRIDGE_BEACON_PROXY_BYTECODE_KEY, L2_LEGACY_SHARED_BRIDGE_L1_ADDRESSES_KEY,
    L2_LEGACY_SHARED_BRIDGE_UPGRADEABLE_BEACON_ADDRESS_KEY, L2_NATIVE_TOKEN_VAULT_ADDRESS,
    L2_NATIVE_TOKEN_VAULT_ASSET_ID_MAPPING_INDEX, U256,
};

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
    storage: &mut Connection<'_, Core>,
    last_sealed_l2_block_number: L2BlockNumber,
    l2_legacy_shared_bridge_address: Address,
    l1_token_address: Address,
) -> anyhow::Result<Address> {
    // The source of truth for this logic is `L2SharedBridgeLegacy._calculateCreate2TokenAddress`

    let beacon_proxy_bytecode_hash_key = StorageKey::new(
        AccountTreeId::new(l2_legacy_shared_bridge_address),
        L2_LEGACY_SHARED_BRIDGE_BEACON_PROXY_BYTECODE_KEY,
    );
    let beacon_proxy_bytecode_hash = storage
        .storage_web3_dal()
        .get_historical_value_unchecked(
            beacon_proxy_bytecode_hash_key.hashed_key(),
            last_sealed_l2_block_number,
        )
        .await?;

    let beacon_address_key = StorageKey::new(
        AccountTreeId::new(l2_legacy_shared_bridge_address),
        L2_LEGACY_SHARED_BRIDGE_UPGRADEABLE_BEACON_ADDRESS_KEY,
    );
    let beacon_address = storage
        .storage_web3_dal()
        .get_historical_value_unchecked(
            beacon_address_key.hashed_key(),
            last_sealed_l2_block_number,
        )
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

#[derive(Debug)]
enum LegacyTokenStatus {
    // In case the deposited token is either not legacy or registered.
    NotLegacyOrRegistered,
    // In case the deposited token is a legacy token that has not
    // been registered
    Legacy,
    // This is the case for an unexpected state where the predicted l1
    // address does not match the storage one. It should never happen, but
    // we return an error instead of a panic for increased liveness / easier debugging,
    // while the transaction will still not be allowed to pass through.
    UnexpectedDifferentL1Address(Address, Address),
}

/// Checks whether the token is legacy. It is legacy if both of the
/// following is true:
/// - It is present in legacy shared bridge
/// - It is not present in the L2 native token vault
async fn is_l2_token_legacy(
    storage: &mut Connection<'_, Core>,
    last_sealed_l2_block_number: L2BlockNumber,
    l2_legacy_shared_bridge_address: Address,
    l2_token_address: Address,
    expected_l1_address: Address,
) -> anyhow::Result<LegacyTokenStatus> {
    // 1. Read l1 token address from L2 shared bridge (must not be 0)

    let stored_l1_address_key = StorageKey::new(
        AccountTreeId::new(l2_legacy_shared_bridge_address),
        get_address_mapping_key(&l2_token_address, L2_LEGACY_SHARED_BRIDGE_L1_ADDRESSES_KEY),
    );
    let stored_l1_address = storage
        .storage_web3_dal()
        .get_historical_value_unchecked(
            stored_l1_address_key.hashed_key(),
            last_sealed_l2_block_number,
        )
        .await?;
    let stored_l1_address = h256_to_address(&stored_l1_address);

    // No address is stored, it means that the token has never been bridged before
    // and thus, it is not legacy
    if stored_l1_address == Address::zero() {
        return Ok(LegacyTokenStatus::NotLegacyOrRegistered);
    }

    if expected_l1_address != stored_l1_address {
        return Ok(LegacyTokenStatus::UnexpectedDifferentL1Address(
            expected_l1_address,
            stored_l1_address,
        ));
    }

    // 2. Read assetId from NTV (must be 0)
    let stored_asset_id_key = StorageKey::new(
        AccountTreeId::new(L2_NATIVE_TOKEN_VAULT_ADDRESS),
        get_address_mapping_key(
            &l2_token_address,
            L2_NATIVE_TOKEN_VAULT_ASSET_ID_MAPPING_INDEX,
        ),
    );
    let stored_asset_id = storage
        .storage_web3_dal()
        .get_historical_value_unchecked(
            stored_asset_id_key.hashed_key(),
            last_sealed_l2_block_number,
        )
        .await?;

    if stored_asset_id == H256::zero() {
        Ok(LegacyTokenStatus::Legacy)
    } else {
        Ok(LegacyTokenStatus::NotLegacyOrRegistered)
    }
}

/// Accepts a list of transactions and returns:
/// - `Some(unsafe_tx_hash)` if there is an unsafe deposit transaction.
/// - `None` if there is no such transaction.  
///
/// Note, that the purpose of this function is not to find unsafe deposits *only*
/// but detect whether they may be present at all. It does check that, e.g. the sender
/// of the transactions is the correct l1 bridge.
pub(crate) async fn find_unsafe_deposit<'a>(
    txs: impl Iterator<Item = &'a Transaction>,
    storage: &mut Connection<'_, Core>,
) -> anyhow::Result<Option<H256>> {
    // The rules are the following:
    // - All L2 transactions are allowed
    // - L1 transactions are allowed only if it is a deposit to an already registered token or
    // to a non-legacy token.

    let last_sealed_l2_block_number = storage
        .blocks_dal()
        .get_sealed_l2_block_number()
        .await?
        .unwrap();

    // Firstly, let's check whether the chain has a legacy bridge.
    let legacy_bridge_key = get_immutable_simulator_key(
        &L2_ASSET_ROUTER_ADDRESS,
        L2_ASSET_ROUTER_LEGACY_SHARED_BRIDGE_IMMUTABLE_KEY,
    );

    let legacy_l2_shared_bridge_addr = storage
        .storage_web3_dal()
        .get_historical_value_unchecked(legacy_bridge_key.hashed_key(), last_sealed_l2_block_number)
        .await?;
    let legacy_l2_shared_bridge_addr = h256_to_address(&legacy_l2_shared_bridge_addr);

    // There is either no legacy bridge or the L2AssetRouter has not been depoyed yet.
    // In both cases, there can be no unsafe deposits.
    if legacy_l2_shared_bridge_addr == Address::zero() {
        return Ok(None);
    }

    // In theory it could be fetched from config, but we do it here for consistency
    let l1_chain_id_key = get_immutable_simulator_key(
        &L2_ASSET_ROUTER_ADDRESS,
        L2_ASSET_ROUTER_LEGACY_SHARED_BRIDGE_L1_CHAIN_ID_KEY,
    );
    let l1_chain_id = storage
        .storage_web3_dal()
        .get_historical_value_unchecked(l1_chain_id_key.hashed_key(), last_sealed_l2_block_number)
        .await?;
    let l1_chain_id = h256_to_u256(l1_chain_id);

    for tx in txs {
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
            extract_token_from_asset_router_deposit(l1_chain_id, tx.execute.calldata())
        } else {
            None
        };

        let Some(l1_token_address) = l1_token_address else {
            continue;
        };

        let l2_token_address = calculate_expected_token_address(
            storage,
            last_sealed_l2_block_number,
            legacy_l2_shared_bridge_addr,
            l1_token_address,
        )
        .await?;

        let token_legacy_status = is_l2_token_legacy(
            storage,
            last_sealed_l2_block_number,
            legacy_l2_shared_bridge_addr,
            l2_token_address,
            l1_token_address,
        )
        .await?;

        match token_legacy_status {
            LegacyTokenStatus::Legacy => {
                return Ok(Some(tx.hash()));
            }
            LegacyTokenStatus::UnexpectedDifferentL1Address(expected, stored) => {
                tracing::error!(
                    "Unexpected stored L1 token for L2 token {:#?}. Expected: {:#?}, Stored: {:#?}",
                    l2_token_address,
                    expected,
                    stored
                );
                // We return this transaction to ensure that it is not processed
                return Ok(Some(tx.hash()));
            }
            LegacyTokenStatus::NotLegacyOrRegistered => {
                // Do nothing.
            }
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
    use zksync_multivm::vm_latest::utils::v26_upgrade::{
        encode_legacy_finalize_deposit, encode_new_finalize_deposit, get_test_data,
    };
    use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
    use zksync_types::{
        Address, Execute, ExecuteTransactionCommon, L2BlockNumber, StorageKey, StorageLog,
        Transaction, H256, L2_ASSET_ROUTER_ADDRESS,
    };

    use crate::v26_utils::find_unsafe_deposit;

    const SIMPLE_TEST_RESULT_JSON: &str = include_str!("./test_v26_utils_outputs/simple-test.json");
    const POST_BRIDGING_TEST_RESULT_JSON: &str =
        include_str!("./test_v26_utils_outputs/post-bridging.json");
    const POST_REGISTRATION_TEST_RESULT_JSON: &str =
        include_str!("./test_v26_utils_outputs/post-registration.json");

    fn trivial_test_storage_logs() -> HashMap<StorageKey, H256> {
        let x: Vec<_> = serde_json::from_str(SIMPLE_TEST_RESULT_JSON).unwrap();
        x.into_iter().collect()
    }

    fn post_bridging_test_storage_logs() -> HashMap<StorageKey, H256> {
        let x: Vec<_> = serde_json::from_str(POST_BRIDGING_TEST_RESULT_JSON).unwrap();
        x.into_iter().collect()
    }

    fn post_registration_test_storage_logs() -> HashMap<StorageKey, H256> {
        let x: Vec<_> = serde_json::from_str(POST_REGISTRATION_TEST_RESULT_JSON).unwrap();
        x.into_iter().collect()
    }

    // Bridging txs can never happen on L2, we use it to
    // just ensure that L2 txs are always allowed.
    fn get_l2_dummy_bridging_tx() -> Transaction {
        let test_data = get_test_data();
        Transaction {
            common_data: ExecuteTransactionCommon::L2(Default::default()),
            execute: Execute {
                contract_address: Some(test_data.l2_legacy_shared_bridge_address),
                calldata: encode_legacy_finalize_deposit(test_data.l1_token_address),
                value: Default::default(),
                factory_deps: vec![],
            },
            received_timestamp_ms: 0,
            raw_bytes: None,
        }
    }

    fn get_l1_bridging_tx() -> Transaction {
        Transaction {
            common_data: ExecuteTransactionCommon::L1(Default::default()),
            ..get_l2_dummy_bridging_tx()
        }
    }

    fn get_l1_new_bridging_tx() -> Transaction {
        let test_data = get_test_data();
        Transaction {
            common_data: ExecuteTransactionCommon::L1(Default::default()),
            execute: Execute {
                contract_address: Some(L2_ASSET_ROUTER_ADDRESS),
                calldata: encode_new_finalize_deposit(
                    test_data.l1_chain_id.0.into(),
                    test_data.l1_token_address,
                ),
                value: Default::default(),
                factory_deps: vec![],
            },
            received_timestamp_ms: 0,
            raw_bytes: None,
        }
    }

    fn get_l1_new_deposit_bad_address() -> Transaction {
        let test_data = get_test_data();
        Transaction {
            common_data: ExecuteTransactionCommon::L1(Default::default()),
            execute: Execute {
                contract_address: Some(Address::from_low_u64_be(1)),
                calldata: encode_new_finalize_deposit(
                    test_data.l1_chain_id.0.into(),
                    test_data.l1_token_address,
                ),
                value: Default::default(),
                factory_deps: vec![],
            },
            received_timestamp_ms: 0,
            raw_bytes: None,
        }
    }

    async fn get_storage(storage_logs: HashMap<StorageKey, H256>) -> Connection<'static, Core> {
        let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();

        let storage_logs: Vec<_> = storage_logs
            .into_iter()
            .map(|(key, value)| StorageLog {
                kind: zksync_types::StorageLogKind::InitialWrite,
                key,
                value,
            })
            .collect();

        storage
            .storage_logs_dal()
            .append_storage_logs(L2BlockNumber(0), &storage_logs)
            .await
            .unwrap();

        storage
    }

    async fn run_test(
        txs: Vec<Transaction>,
        expected_hash: Option<H256>,
        storage: &mut Connection<'static, Core>,
    ) {
        assert_eq!(
            find_unsafe_deposit(txs.iter(), storage).await.unwrap(),
            expected_hash
        );
    }

    #[tokio::test]
    async fn test_is_unsafe_deposit_post_bridging() {
        let mut storage = get_storage(post_bridging_test_storage_logs()).await;

        run_test(vec![], None, &mut storage).await;
        run_test(vec![get_l2_dummy_bridging_tx()], None, &mut storage).await;
        run_test(
            vec![get_l2_dummy_bridging_tx(), get_l1_bridging_tx()],
            None,
            &mut storage,
        )
        .await;
        run_test(vec![get_l1_new_bridging_tx()], None, &mut storage).await;
        run_test(vec![get_l1_new_deposit_bad_address()], None, &mut storage).await;
    }

    #[tokio::test]
    async fn test_is_unsafe_deposit_post_registration() {
        let mut storage = get_storage(post_registration_test_storage_logs()).await;

        run_test(vec![], None, &mut storage).await;
        run_test(vec![get_l2_dummy_bridging_tx()], None, &mut storage).await;
        run_test(
            vec![get_l2_dummy_bridging_tx(), get_l1_bridging_tx()],
            None,
            &mut storage,
        )
        .await;
        run_test(vec![get_l1_new_bridging_tx()], None, &mut storage).await;
        run_test(vec![get_l1_new_deposit_bad_address()], None, &mut storage).await;
    }

    #[tokio::test]
    async fn test_is_unsafe_deposit_trivial_case() {
        let mut storage = get_storage(trivial_test_storage_logs()).await;

        run_test(vec![], None, &mut storage).await;
        run_test(vec![get_l2_dummy_bridging_tx()], None, &mut storage).await;
        run_test(
            vec![get_l2_dummy_bridging_tx(), get_l1_bridging_tx()],
            Some(get_l1_bridging_tx().hash()),
            &mut storage,
        )
        .await;
        run_test(
            vec![get_l1_new_bridging_tx()],
            Some(get_l1_new_bridging_tx().hash()),
            &mut storage,
        )
        .await;
        run_test(vec![get_l1_new_deposit_bad_address()], None, &mut storage).await;
    }
}

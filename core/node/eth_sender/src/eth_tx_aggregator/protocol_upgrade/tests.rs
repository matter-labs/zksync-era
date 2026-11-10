use std::{
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use zksync_config::{
    configs::eth_sender::{ProofSendingMode, ProverType, SenderConfig},
    EthConfig,
};
use zksync_contracts::{
    server_notifier_contract, state_transition_manager_contract,
    UPGRADE_CHAIN_FROM_VERSION_FUNCTION,
};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_node_test_utils::create_l1_batch;
use zksync_object_store::{MockObjectStore, ObjectStore};
use zksync_prover_interface::outputs::{
    AirbenderL1BatchProofForL1, L1BatchProofForL1, L1BatchProofForL1Key,
};
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    commitment::L1BatchCommitmentMode,
    ethabi::{self, Token},
    protocol_version::{ProtocolSemanticVersion, VersionPatch},
    settlement::SettlementLayer,
    web3::{self, keccak256},
    Address, L1BatchNumber, ProtocolVersion, ProtocolVersionId, H256, U256,
};
use zksync_web3_decl::jsonrpsee::core::ClientError;

use crate::{
    aggregator::{Aggregator, OperationSkippingRestrictions},
    tester::{
        EthSenderTester, TestL1Batch, STATE_TRANSITION_CONTRACT_ADDRESS,
        STATE_TRANSITION_MANAGER_CONTRACT_ADDRESS,
    },
    tests::ENFORCING_VALIDATOR_TIMELOCK_ADDRESS,
};

const SERVER_NOTIFIER_ADDRESS: Address = Address::repeat_byte(0xc0);

#[derive(Debug)]
struct MockUpgradeState {
    upgrade_timestamp: U256,
    upgrade_cut_hash: H256,
    simulation_reverts: bool,
}

fn upgrade_cut_data() -> Vec<u8> {
    ethabi::encode(&[Token::Tuple(vec![
        Token::Array(vec![]),
        Token::Address(Address::repeat_byte(0x11)),
        Token::Bytes(vec![1, 2, 3]),
    ])])
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn current_version() -> ProtocolSemanticVersion {
    ProtocolSemanticVersion {
        minor: ProtocolVersionId::latest(),
        patch: VersionPatch(0),
    }
}

fn call_handler(
    state: Arc<Mutex<MockUpgradeState>>,
) -> impl Fn(&web3::CallRequest) -> Result<Token, ClientError> + Send + Sync {
    let ctm = state_transition_manager_contract();
    let server_notifier_address_selector = ctm
        .function("serverNotifierAddress")
        .unwrap()
        .short_signature();
    let upgrade_cut_hash_selector = ctm.function("upgradeCutHash").unwrap().short_signature();
    let upgrade_timestamp_selector = server_notifier_contract()
        .function("protocolVersionToUpgradeTimestamp")
        .unwrap()
        .short_signature();

    move |call| {
        let state = state.lock().unwrap();
        let to = call.to.unwrap();
        let selector = &call.data.as_ref().unwrap().0[..4];
        if to == STATE_TRANSITION_MANAGER_CONTRACT_ADDRESS
            && selector == server_notifier_address_selector
        {
            Ok(Token::Address(SERVER_NOTIFIER_ADDRESS))
        } else if to == STATE_TRANSITION_MANAGER_CONTRACT_ADDRESS
            && selector == upgrade_cut_hash_selector
        {
            Ok(Token::FixedBytes(state.upgrade_cut_hash.0.to_vec()))
        } else if to == SERVER_NOTIFIER_ADDRESS && selector == upgrade_timestamp_selector {
            Ok(Token::Uint(state.upgrade_timestamp))
        } else if to == ENFORCING_VALIDATOR_TIMELOCK_ADDRESS {
            assert_eq!(
                selector,
                UPGRADE_CHAIN_FROM_VERSION_FUNCTION.short_signature()
            );
            if state.simulation_reverts {
                Err(ClientError::Custom("execution reverted".to_owned()))
            } else {
                Ok(Token::Bytes(vec![]))
            }
        } else {
            panic!("unexpected call: {call:?}")
        }
    }
}

async fn create_tester(
    pool: ConnectionPool<Core>,
    new_version: ProtocolSemanticVersion,
    save_cut: bool,
) -> (EthSenderTester, Arc<Mutex<MockUpgradeState>>) {
    let state = Arc::new(Mutex::new(MockUpgradeState {
        upgrade_timestamp: now().into(),
        upgrade_cut_hash: H256(keccak256(&upgrade_cut_data())),
        simulation_reverts: false,
    }));
    let tester = EthSenderTester::new_with_call_handler(
        pool,
        vec![100; 100],
        false,
        false,
        L1BatchCommitmentMode::Rollup,
        SettlementLayer::for_tests(),
        ProtocolVersionId::latest(),
        Arc::new(call_handler(state.clone())),
    )
    .await;

    let mut storage = tester.storage().await;
    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion {
            version: new_version,
            ..Default::default()
        })
        .await
        .unwrap();
    if save_cut {
        storage
            .protocol_versions_dal()
            .save_upgrade_cut_data(new_version, &upgrade_cut_data())
            .await
            .unwrap();
    }
    drop(storage);
    (tester, state)
}

async fn handle_upgrade(
    tester: &mut EthSenderTester,
    restrictions: &mut OperationSkippingRestrictions,
) -> bool {
    let mut storage = tester.conn.connection().await.unwrap();
    tester
        .aggregator
        .handle_protocol_upgrade(
            &mut storage,
            current_version(),
            ENFORCING_VALIDATOR_TIMELOCK_ADDRESS,
            false,
            restrictions,
        )
        .await
        .unwrap()
}

async fn upgrade_txs_count(tester: &EthSenderTester) -> i64 {
    tester
        .storage()
        .await
        .eth_sender_dal()
        .count_eth_txs_by_type(AggregatedActionType::ProtocolUpgrade)
        .await
}

async fn seal_batch_with_version(
    tester: &EthSenderTester,
    number: u32,
    version: ProtocolVersionId,
) {
    let mut header = create_l1_batch(number);
    header.protocol_version = Some(version);
    tester
        .storage()
        .await
        .blocks_dal()
        .insert_mock_l1_batch(&header)
        .await
        .unwrap();
}

#[tokio::test]
async fn minor_upgrade_is_sent_once_previous_version_batches_are_proven() {
    let new_version = ProtocolSemanticVersion {
        minor: ProtocolVersionId::next(),
        patch: VersionPatch(0),
    };
    let (mut tester, _) = create_tester(ConnectionPool::test_pool().await, new_version, true).await;
    let mut restrictions = OperationSkippingRestrictions::default();

    TestL1Batch::sealed(&mut tester).await;
    let l1_batch = TestL1Batch::sealed(&mut tester).await;
    l1_batch.commit(&mut tester, true).await;

    // The batch of the new version is not sealed yet.
    assert!(!handle_upgrade(&mut tester, &mut restrictions).await);

    seal_batch_with_version(&tester, 2, new_version.minor).await;
    // The last batch of the previous version is not proven yet.
    assert!(!handle_upgrade(&mut tester, &mut restrictions).await);

    l1_batch.prove(&mut tester, true).await;
    assert!(handle_upgrade(&mut tester, &mut restrictions).await);
    assert_eq!(upgrade_txs_count(&tester).await, 1);
    assert!(restrictions.commit_restriction.is_none());

    let tx = tester
        .storage()
        .await
        .eth_sender_dal()
        .get_eth_tx(3)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(tx.tx_type, AggregatedActionType::ProtocolUpgrade);
    assert_eq!(tx.contract_address, ENFORCING_VALIDATOR_TIMELOCK_ADDRESS);
    let mut cut = ethabi::decode(
        &[UPGRADE_CHAIN_FROM_VERSION_FUNCTION.inputs[2].kind.clone()],
        &upgrade_cut_data(),
    )
    .unwrap();
    let expected_calldata = UPGRADE_CHAIN_FROM_VERSION_FUNCTION
        .encode_input(&[
            Token::Address(STATE_TRANSITION_CONTRACT_ADDRESS),
            Token::Uint(current_version().pack()),
            cut.remove(0),
        ])
        .unwrap();
    assert_eq!(tx.raw_tx, expected_calldata);

    // Other operations wait until the upgrade tx is confirmed.
    assert!(!handle_upgrade(&mut tester, &mut restrictions).await);
    assert_eq!(upgrade_txs_count(&tester).await, 1);
    assert!(restrictions.commit_restriction.is_some());
    assert!(restrictions.prove_restriction.is_some());
    assert!(restrictions.execute_restriction.is_some());
    assert!(restrictions.precommit_restriction.is_some());
}

#[tokio::test]
async fn patch_upgrade_waits_for_inflight_proofs() {
    let new_version = ProtocolSemanticVersion {
        minor: ProtocolVersionId::latest(),
        patch: VersionPatch(1),
    };
    let (mut tester, _) = create_tester(ConnectionPool::test_pool().await, new_version, true).await;
    let mut restrictions = OperationSkippingRestrictions::default();

    TestL1Batch::sealed(&mut tester).await;
    let l1_batch = TestL1Batch::sealed(&mut tester).await;
    l1_batch.commit(&mut tester, true).await;
    l1_batch.prove(&mut tester, false).await;
    assert!(!handle_upgrade(&mut tester, &mut restrictions).await);

    l1_batch.execute_prove_tx(&mut tester).await;
    tester.run_eth_sender_tx_manager_iteration().await;
    assert!(handle_upgrade(&mut tester, &mut restrictions).await);
    assert_eq!(upgrade_txs_count(&tester).await, 1);
}

async fn store_proof(
    blob_store: &dyn ObjectStore,
    l1_batch_number: L1BatchNumber,
    version: ProtocolSemanticVersion,
) {
    let proof = L1BatchProofForL1::new_airbender(AirbenderL1BatchProofForL1 {
        proof: vec![1, 2, 3],
        protocol_version: version,
    });
    blob_store
        .put(
            L1BatchProofForL1Key::Core((l1_batch_number, version)),
            &proof,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn patch_upgrade_waits_for_new_version_proof() {
    let new_version = ProtocolSemanticVersion {
        minor: ProtocolVersionId::latest(),
        patch: VersionPatch(1),
    };
    let (mut tester, _) = create_tester(ConnectionPool::test_pool().await, new_version, true).await;
    let blob_store = MockObjectStore::arc();
    tester.aggregator.aggregator = Aggregator::new(
        SenderConfig {
            proof_sending_mode: ProofSendingMode::OnlyRealProofs,
            prover: ProverType::Boojum,
            ..EthConfig::for_tests()
                .get_eth_sender_config_for_sender_layer_data_layer()
                .clone()
        },
        blob_store.clone(),
        false,
        L1BatchCommitmentMode::Rollup,
        tester.conn.clone(),
        SettlementLayer::for_tests(),
    )
    .await
    .unwrap();
    let mut restrictions = OperationSkippingRestrictions::default();

    TestL1Batch::sealed(&mut tester).await;
    let l1_batch = TestL1Batch::sealed(&mut tester).await;
    // The batch isn't committed yet, but its proof is already generated for the current version.
    store_proof(&*blob_store, l1_batch.number, current_version()).await;
    assert!(!handle_upgrade(&mut tester, &mut restrictions).await);

    store_proof(&*blob_store, l1_batch.number, new_version).await;
    assert!(handle_upgrade(&mut tester, &mut restrictions).await);
    assert_eq!(upgrade_txs_count(&tester).await, 1);
}

#[tokio::test]
async fn upgrade_is_not_sent_if_l1_rejects_it() {
    let new_version = ProtocolSemanticVersion {
        minor: ProtocolVersionId::latest(),
        patch: VersionPatch(1),
    };
    let (mut tester, state) =
        create_tester(ConnectionPool::test_pool().await, new_version, true).await;
    let mut restrictions = OperationSkippingRestrictions::default();

    state.lock().unwrap().upgrade_timestamp = U256::zero();
    assert!(!handle_upgrade(&mut tester, &mut restrictions).await);

    state.lock().unwrap().upgrade_timestamp = (now() + 3600).into();
    assert!(!handle_upgrade(&mut tester, &mut restrictions).await);

    state.lock().unwrap().upgrade_timestamp = now().into();
    state.lock().unwrap().upgrade_cut_hash = H256::repeat_byte(1);
    assert!(!handle_upgrade(&mut tester, &mut restrictions).await);

    state.lock().unwrap().upgrade_cut_hash = H256(keccak256(&upgrade_cut_data()));
    state.lock().unwrap().simulation_reverts = true;
    assert!(!handle_upgrade(&mut tester, &mut restrictions).await);
    assert_eq!(upgrade_txs_count(&tester).await, 0);

    state.lock().unwrap().simulation_reverts = false;
    assert!(handle_upgrade(&mut tester, &mut restrictions).await);
    assert_eq!(upgrade_txs_count(&tester).await, 1);
}

#[tokio::test]
async fn upgrade_without_stored_cut_is_skipped() {
    let new_version = ProtocolSemanticVersion {
        minor: ProtocolVersionId::latest(),
        patch: VersionPatch(1),
    };
    let (mut tester, _) =
        create_tester(ConnectionPool::test_pool().await, new_version, false).await;
    let mut restrictions = OperationSkippingRestrictions::default();

    assert!(!handle_upgrade(&mut tester, &mut restrictions).await);
    assert_eq!(upgrade_txs_count(&tester).await, 0);
}

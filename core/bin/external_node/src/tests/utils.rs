use std::sync::Arc;

use tempfile::TempDir;
use tokio::sync::oneshot;
use zksync_dal::{ConnectionPoolBuilder, Core, CoreDal};
use zksync_db_connection::connection_pool::TestTemplate;
use zksync_eth_client::clients::MockSettlementLayer;
use zksync_health_check::AppHealthCheck;
use zksync_node_genesis::{insert_genesis_batch, GenesisBatchParams, GenesisParams};
use zksync_types::{
    api, block::L2BlockHeader, ethabi, Address, L2BlockNumber, ProtocolVersionId, H256,
};
use zksync_web3_decl::client::{MockClient, L1};

use super::*;

pub(super) fn block_details_base(hash: H256) -> api::BlockDetailsBase {
    api::BlockDetailsBase {
        timestamp: 0,
        l1_tx_count: 0,
        l2_tx_count: 0,
        root_hash: Some(hash),
        status: api::BlockStatus::Sealed,
        commit_tx_hash: None,
        committed_at: None,
        commit_chain_id: None,
        prove_tx_hash: None,
        proving_started_at: None,
        proven_at: None,
        prove_chain_id: None,
        execute_tx_hash: None,
        executed_at: None,
        execute_chain_id: None,
        l1_gas_price: 0,
        l2_fair_gas_price: 0,
        fair_pubdata_price: None,
        base_system_contracts_hashes: Default::default(),
    }
}

#[derive(Debug)]
pub(super) struct TestEnvironment {
    pub(super) sigint_receiver: oneshot::Receiver<()>,
    pub(super) app_health_sender: oneshot::Sender<Arc<AppHealthCheck>>,
    pub(super) components: ComponentsToRun,
    pub(super) config: ExternalNodeConfig,
    pub(super) genesis_params: GenesisBatchParams,
    pub(super) genesis_l2_block: L2BlockHeader,
    // We have to prevent object from dropping the temp dir, so we store it here.
    _temp_dir: TempDir,
}

impl TestEnvironment {
    pub async fn with_genesis_block(components_str: &str) -> (Self, TestEnvironmentHandles) {
        // Generate a new environment with a genesis block.
        let temp_dir = tempfile::TempDir::new().unwrap();

        // Simplest case to mock: the EN already has a genesis L1 batch / L2 block, and it's the only L1 batch / L2 block
        // in the network.
        let test_db: ConnectionPoolBuilder<Core> =
            TestTemplate::empty().unwrap().create_db(100).await.unwrap();
        let connection_pool = test_db.build().await.unwrap();
        // let singleton_pool_builder = ConnectionPool::singleton(connection_pool.database_url().clone());
        let mut storage = connection_pool.connection().await.unwrap();
        let genesis_params = insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
        let genesis_l2_block = storage
            .blocks_dal()
            .get_l2_block_header(L2BlockNumber(0))
            .await
            .unwrap()
            .expect("No genesis L2 block");
        drop(storage);

        let components: ComponentsToRun = components_str.parse().unwrap();
        let mut config = ExternalNodeConfig::mock(&temp_dir, &connection_pool);
        if components.0.contains(&Component::TreeApi) {
            config.tree_component.api_port = Some(0);
        }
        drop(connection_pool);

        // Generate channels to control the node.

        let (sigint_sender, sigint_receiver) = oneshot::channel();
        let (app_health_sender, app_health_receiver) = oneshot::channel();
        let this = Self {
            sigint_receiver,
            app_health_sender,
            components,
            config,
            genesis_params,
            genesis_l2_block,
            _temp_dir: temp_dir,
        };
        let handles = TestEnvironmentHandles {
            sigint_sender,
            app_health_receiver,
        };

        (this, handles)
    }
}

#[derive(Debug)]
pub(super) struct TestEnvironmentHandles {
    pub(super) sigint_sender: oneshot::Sender<()>,
    pub(super) app_health_receiver: oneshot::Receiver<Arc<AppHealthCheck>>,
}

// The returned components have the fully implemented health check life cycle (i.e., signal their shutdown).
pub(super) fn expected_health_components(components: &ComponentsToRun) -> Vec<&'static str> {
    let mut output = vec!["reorg_detector"];
    if components.0.contains(&Component::Core) {
        output.extend(["consistency_checker", "commitment_generator"]);
    }
    if components.0.contains(&Component::Tree) {
        output.push("tree");
    }
    if components.0.contains(&Component::HttpApi) {
        output.push("http_api");
    }
    if components.0.contains(&Component::WsApi) {
        output.push("ws_api");
    }
    output
}

pub(super) fn mock_eth_client(
    diamond_proxy_addr: Address,
    bridgehub_addres: Address,
) -> MockClient<L1> {
    let chain_type_manager = Address::repeat_byte(16);
    let message_root_proxy_addr = Address::repeat_byte(17);
    let mock = MockSettlementLayer::builder().with_call_handler(move |call, _| {
        tracing::info!("L1 call: {call:?}");
        if call.to == Some(diamond_proxy_addr) {
            let packed_semver = ProtocolVersionId::latest().into_packed_semver_with_patch(0);
            let call_signature = &call.data.as_ref().unwrap().0[..4];
            let contract = zksync_contracts::hyperchain_contract();
            let pricing_mode_sig = contract
                .function("getPubdataPricingMode")
                .unwrap()
                .short_signature();
            let protocol_version_sig = contract
                .function("getProtocolVersion")
                .unwrap()
                .short_signature();
            let settlement_layer_sig = contract
                .function("getSettlementLayer")
                .unwrap()
                .short_signature();
            match call_signature {
                sig if sig == pricing_mode_sig => {
                    return ethabi::Token::Uint(0.into()); // "rollup" mode encoding
                }
                sig if sig == protocol_version_sig => return ethabi::Token::Uint(packed_semver),
                sig if sig == settlement_layer_sig => return ethabi::Token::Uint(0.into()),
                _ => { /* unknown call; panic below */ }
            }
        } else if call.to == Some(bridgehub_addres) {
            let call_signature = &call.data.as_ref().unwrap().0[..4];
            let contract = zksync_contracts::bridgehub_contract();
            let get_zk_chains = contract.function("getZKChain").unwrap().short_signature();
            let chain_type_manager_sig = contract
                .function("chainTypeManager")
                .unwrap()
                .short_signature();
            let message_root_sig = contract.function("messageRoot").unwrap().short_signature();

            match call_signature {
                sig if sig == get_zk_chains => {
                    return ethabi::Token::Address(diamond_proxy_addr);
                }
                sig if sig == chain_type_manager_sig => {
                    return ethabi::Token::Address(chain_type_manager);
                }
                sig if sig == message_root_sig => {
                    return ethabi::Token::Address(message_root_proxy_addr);
                }
                _ => {}
            }
        } else if call.to == Some(chain_type_manager) {
            return ethabi::Token::Address(Address::random());
        }

        panic!("Unexpected L1 call: {call:?}");
    });
    mock.build().into_client()
}

/// Creates a mock L2 client with the genesis block information.
pub(super) fn mock_l2_client(env: &TestEnvironment) -> MockClient<L2> {
    let genesis_root_hash = env.genesis_params.root_hash;
    let genesis_l2_block_hash = env.genesis_l2_block.hash;

    MockClient::builder(L2::default())
        .method("eth_chainId", || Ok(U64::from(270)))
        .method("zks_L1ChainId", || Ok(U64::from(9)))
        .method("zks_L1BatchNumber", || Ok(U64::from(0)))
        .method("zks_getL1BatchDetails", move |number: L1BatchNumber| {
            assert_eq!(number, L1BatchNumber(0));
            Ok(api::L1BatchDetails {
                number: L1BatchNumber(0),
                base: utils::block_details_base(genesis_root_hash),
            })
        })
        .method("eth_blockNumber", || Ok(U64::from(0)))
        .method(
            "eth_getBlockByNumber",
            move |number: api::BlockNumber, _with_txs: bool| {
                assert_eq!(number, api::BlockNumber::Number(0.into()));
                Ok(api::Block::<api::TransactionVariant> {
                    hash: genesis_l2_block_hash,
                    ..api::Block::default()
                })
            },
        )
        .method("zks_getFeeParams", || Ok(FeeParams::sensible_v1_default()))
        .method("en_whitelistedTokensForAA", || Ok([] as [Address; 0]))
        .build()
}

/// Creates a mock L2 client that will mimic request timeouts on block info requests.
pub(super) fn mock_l2_client_hanging() -> MockClient<L2> {
    MockClient::builder(L2::default())
        .method("eth_chainId", || Ok(U64::from(270)))
        .method("zks_L1ChainId", || Ok(U64::from(9)))
        .method("zks_L1BatchNumber", || {
            Err::<(), _>(ClientError::RequestTimeout)
        })
        .method("eth_blockNumber", || {
            Err::<(), _>(ClientError::RequestTimeout)
        })
        .method("zks_getFeeParams", || Ok(FeeParams::sensible_v1_default()))
        .method("en_whitelistedTokensForAA", || Ok([] as [Address; 0]))
        .build()
}

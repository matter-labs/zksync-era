use std::sync::Arc;

use tempfile::TempDir;
use tokio::sync::oneshot;
use zksync_config::configs::contracts::{
    chain::ChainContracts, ecosystem::EcosystemCommonContracts, SettlementLayerSpecificContracts,
};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_eth_client::clients::MockSettlementLayer;
use zksync_health_check::AppHealthCheck;
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_types::{
    api, api::BridgeAddresses, block::L2BlockHeader, ethabi, Address, L2BlockNumber,
    ProtocolVersionId, H256,
};
use zksync_web3_decl::{
    client::{MockClient, L1, L2},
    types::EcosystemContractsDto,
};

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
        commit_tx_finality: None,
        commit_chain_id: None,
        prove_tx_hash: None,
        prove_tx_finality: None,
        proven_at: None,
        prove_chain_id: None,
        execute_tx_hash: None,
        execute_tx_finality: None,
        executed_at: None,
        execute_chain_id: None,
        precommit_tx_hash: None,
        precommit_tx_finality: None,
        precommitted_at: None,
        precommit_chain_id: None,
        l1_gas_price: 0,
        l2_fair_gas_price: 0,
        fair_pubdata_price: None,
        base_system_contracts_hashes: Default::default(),
    }
}

pub(super) fn spawn_node(
    node_fn: impl FnOnce() -> anyhow::Result<()> + Send + 'static,
) -> JoinHandle<anyhow::Result<()>> {
    tokio::task::spawn_blocking(|| std::thread::spawn(node_fn).join().unwrap())
}

#[derive(Debug)]
pub(super) struct TestEnvironment {
    pub(super) sigint_receiver: oneshot::Receiver<()>,
    pub(super) app_health_sender: oneshot::Sender<Arc<AppHealthCheck>>,
    pub(super) components: ComponentsToRun,
    pub(super) config: ExternalNodeConfig,
    pub(super) genesis_root_hash: H256,
    pub(super) genesis_l2_block: L2BlockHeader,
    pub(super) settlement_layer_specific_contracts: SettlementLayerSpecificContracts,
}

impl TestEnvironment {
    pub async fn with_genesis_block(
        temp_dir: &TempDir,
        connection_pool: &ConnectionPool<Core>,
        components_str: &str,
    ) -> (Self, TestEnvironmentHandles) {
        // Simplest case to mock: the EN already has a genesis L1 batch / L2 block, and it's the only L1 batch / L2 block
        // in the network.
        let mut storage = connection_pool.connection().await.unwrap();
        let genesis_needed = storage.blocks_dal().is_genesis_needed().await.unwrap();
        let genesis_root_hash = if genesis_needed {
            insert_genesis_batch(&mut storage, &GenesisParams::mock())
                .await
                .unwrap()
                .root_hash
        } else {
            storage
                .blocks_dal()
                .get_l1_batch_state_root(L1BatchNumber(0))
                .await
                .unwrap()
                .expect("no genesis batch root hash")
        };
        let genesis_l2_block = storage
            .blocks_dal()
            .get_l2_block_header(L2BlockNumber(0))
            .await
            .unwrap()
            .expect("No genesis L2 block");
        drop(storage);

        let components: ComponentsToRun = components_str.parse().unwrap();
        let mut config = ExternalNodeConfig::mock(temp_dir, connection_pool);
        if components.0.contains(&Component::TreeApi) {
            config.local.api.merkle_tree.port = 0;
        }

        // Generate channels to control the node.
        let (sigint_sender, sigint_receiver) = oneshot::channel();
        let (app_health_sender, app_health_receiver) = oneshot::channel();
        let this = Self {
            sigint_receiver,
            app_health_sender,
            components,
            config,
            genesis_root_hash,
            genesis_l2_block,
            settlement_layer_specific_contracts: SettlementLayerSpecificContracts {
                ecosystem_contracts: EcosystemCommonContracts {
                    bridgehub_proxy_addr: Some(Address::repeat_byte(8)),
                    state_transition_proxy_addr: None,
                    message_root_proxy_addr: None,
                    multicall3: None,
                    validator_timelock_addr: None,
                },
                chain_contracts_config: ChainContracts {
                    diamond_proxy_addr: Address::repeat_byte(1),
                },
            },
        };
        let handles = TestEnvironmentHandles {
            sigint_sender,
            app_health_receiver,
        };

        (this, handles)
    }

    pub(super) fn spawn_node(self, l2_client: MockClient<L2>) -> JoinHandle<anyhow::Result<()>> {
        let eth_client = mock_eth_client(
            self.settlement_layer_specific_contracts
                .chain_contracts_config
                .diamond_proxy_addr,
            self.settlement_layer_specific_contracts
                .ecosystem_contracts
                .bridgehub_proxy_addr
                .unwrap(),
        );

        spawn_node(move || {
            let mut node = ExternalNodeBuilder::new(self.config)?;
            inject_test_layers(
                &mut node,
                self.sigint_receiver,
                self.app_health_sender,
                eth_client,
                l2_client,
            );

            let node = node.build(self.components.0.into_iter().collect())?;
            node.run(())?;
            Ok(())
        })
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

            let whitelisted_settlement_layer_sig = contract
                .function("whitelistedSettlementLayers")
                .unwrap()
                .short_signature();

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
                sig if sig == whitelisted_settlement_layer_sig => {
                    return ethabi::Token::Bool(false);
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
    let genesis_root_hash = env.genesis_root_hash;
    let genesis_l2_block_hash = env.genesis_l2_block.hash;

    let contracts = env.settlement_layer_specific_contracts.clone();
    MockClient::builder(L2::default())
        .method("eth_chainId", || Ok(U64::from(270)))
        .method("zks_L1ChainId", || Ok(U64::from(9)))
        .method("zks_L1BatchNumber", || Ok(U64::from(0)))
        .method("zks_getL1BatchDetails", move |number: L1BatchNumber| {
            assert_eq!(number, L1BatchNumber(0));
            Ok(api::L1BatchDetails {
                number: L1BatchNumber(0),
                commitment: Some(H256::zero()),
                base: utils::block_details_base(genesis_root_hash),
            })
        })
        .method("eth_blockNumber", || Ok(U64::from(0)))
        .method(
            "eth_getBlockByNumber",
            move |number: api::BlockNumber, _with_txs: bool| {
                match number {
                    api::BlockNumber::Number(num) => assert_eq!(num, U64::from(0)),
                    _ => {} // request for latest, finalized etc. are fine and can return genesis
                }
                Ok(api::Block::<api::TransactionVariant> {
                    hash: genesis_l2_block_hash,
                    ..api::Block::default()
                })
            },
        )
        .method("zks_getFeeParams", || Ok(FeeParams::sensible_v1_default()))
        .method("en_whitelistedTokensForAA", || Ok([] as [Address; 0]))
        .method("en_getEcosystemContracts", move || {
            Ok(EcosystemContractsDto {
                bridgehub_proxy_addr: contracts.ecosystem_contracts.bridgehub_proxy_addr.unwrap(),
                state_transition_proxy_addr: contracts
                    .ecosystem_contracts
                    .state_transition_proxy_addr,
                message_root_proxy_addr: contracts.ecosystem_contracts.message_root_proxy_addr,
                transparent_proxy_admin_addr: Default::default(),
                l1_bytecodes_supplier_addr: None,
                l1_wrapped_base_token_store: None,
                server_notifier_addr: None,
            })
        })
        .method("zks_getBridgeContracts", || {
            Ok(BridgeAddresses {
                l1_shared_default_bridge: Some(Address::repeat_byte(1)),
                l2_shared_default_bridge: Some(Address::repeat_byte(3)),
                l1_erc20_default_bridge: Some(Address::repeat_byte(4)),
                l2_erc20_default_bridge: Some(Address::repeat_byte(3)),
                l1_weth_bridge: None,
                l2_weth_bridge: None,
                l2_legacy_shared_bridge: Some(Address::repeat_byte(5)),
            })
        })
        .method("zks_getTestnetPaymaster", || Ok(Address::repeat_byte(15)))
        .method("zks_getMainContract", move || {
            Ok(contracts.chain_contracts_config.diamond_proxy_addr)
        })
        .build()
}

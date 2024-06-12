#![allow(unused)]

use std::time::Duration;

use anyhow::Context as _;
use multivm::circuit_sequencer_api_latest::boojum::pairing::hex;
use test_casing::test_casing;
use tracing::Instrument as _;
use zksync_concurrency::{ctx, scope, time};
use zksync_config::configs::consensus::{ValidatorPublicKey, WeightedValidator};
use zksync_consensus_crypto::TextFmt as _;
use zksync_consensus_network::testonly::{new_configs, new_fullnode};
use zksync_consensus_roles::{
    validator,
    validator::testonly::{Setup, SetupSpec},
};
use zksync_contracts::load_contract;
use zksync_dal::CoreDal;
use zksync_node_api_server::execution_sandbox::BlockStartInfo;
use zksync_node_test_utils::Snapshot;
use zksync_state_keeper::testonly::{fee, l2_transaction};
use zksync_system_constants::get_intrinsic_constants;
use zksync_test_account::{Account, TxType};
use zksync_types::{
    api::{BlockId, BlockNumber},
    fee::Fee,
    l2::L2Tx,
    transaction_request::{CallOverrides, PaymasterParams},
    utils::deployed_address_create,
    Address, Execute, L1BatchNumber, L2BlockNumber, L2ChainId, Nonce, H256, U256,
};

use super::*;

async fn new_pool(from_snapshot: bool) -> ConnectionPool {
    match from_snapshot {
        true => {
            ConnectionPool::from_snapshot(Snapshot::make(L1BatchNumber(23), L2BlockNumber(87), &[]))
                .await
        }
        false => ConnectionPool::from_genesis().await,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_validator_block_store() {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();
    let pool = new_pool(false).await;

    // Fill storage with unsigned L2 blocks.
    // Fetch a suffix of blocks that we will generate (fake) certs for.
    let want = scope::run!(ctx, |ctx, s| async {
        // Start state keeper.
        let (mut sk, runner) = testonly::StateKeeper::new(ctx, pool.clone()).await?;
        s.spawn_bg(runner.run(ctx));
        sk.push_random_blocks(rng, 10).await;
        pool.wait_for_payload(ctx, sk.last_block()).await?;
        let mut setup = SetupSpec::new(rng, 3);
        setup.first_block = validator::BlockNumber(4);
        let mut setup = Setup::from(setup);
        let mut conn = pool.connection(ctx).await.wrap("connection()")?;
        conn.try_update_genesis(ctx, &setup.genesis)
            .await
            .wrap("try_update_genesis()")?;
        for i in setup.genesis.first_block.0..sk.last_block().next().0 {
            let i = validator::BlockNumber(i);
            let payload = conn
                .payload(ctx, i)
                .await
                .wrap(i)?
                .with_context(|| format!("payload for {i:?} not found"))?
                .encode();
            setup.push_block(payload);
        }
        Ok(setup.blocks.clone())
    })
    .await
    .unwrap();

    // Insert blocks one by one and check the storage state.
    for (i, block) in want.iter().enumerate() {
        scope::run!(ctx, |ctx, s| async {
            let (store, runner) = Store::new(ctx, pool.clone(), None).await.unwrap();
            s.spawn_bg(runner.run(ctx));
            let (block_store, runner) =
                BlockStore::new(ctx, Box::new(store.clone())).await.unwrap();
            s.spawn_bg(runner.run(ctx));
            block_store.queue_block(ctx, block.clone()).await.unwrap();
            block_store
                .wait_until_persisted(ctx, block.number())
                .await
                .unwrap();
            let got = pool
                .wait_for_certificates(ctx, block.number())
                .await
                .unwrap();
            assert_eq!(want[..=i], got);
            Ok(())
        })
        .await
        .unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_validator_validator_registry() {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();

    scope::run!(ctx, |ctx, s| async {
        let pool = ConnectionPool::from_genesis().await;
        let (mut node, runner) = testonly::StateKeeper::new(ctx, pool.clone()).await?;
        let mut account = runner.account.clone();
        s.spawn_bg(runner.run_real(ctx));

        let tx_executor = zksync_node_api_server::execution_sandbox::TransactionExecutor::Real;
        let l2_chain_id = L2ChainId::default();
        let (tx_sender, _) = zksync_node_api_server::web3::testonly::create_test_tx_sender(pool.0.clone(), l2_chain_id, tx_executor).await;

        // Deploy contract 

        // let code = zksync_contracts::read_bytecode("contracts/l2-contracts/artifacts-zk/contracts/ValidatorRegistry.sol/ValidatorRegistry.json");
        /* 
        pragma solidity ^0.8.0;

        contract ValidatorRegistry {
            address public owner;
            mapping(address => bool) public validators;
        
            event ValidatorAdded(address indexed validator);
            event ValidatorRemoved(address indexed validator);
        
            constructor() {
                owner = msg.sender;
            }
        
            modifier onlyOwner() {
                require(msg.sender == owner, "Only owner can call this function");
                _;
            }
        
            function getOwner() public view returns (address) {
                return owner;
            }
        
            // Add a new validator to the list
            function addValidator(address _validator) public onlyOwner {
                require(!validators[_validator], "Validator already exists");
                validators[_validator] = true;
                emit ValidatorAdded(_validator);
            }
        
            // Remove a validator from the list
            function removeValidator(address _validator) public onlyOwner {
                require(validators[_validator], "Validator does not exist");
                validators[_validator] = false;
                emit ValidatorRemoved(_validator);
            }
        
            // Rotate the validator list for a new epoch
            function rotateValidators(address[] memory _newValidators) public onlyOwner {
                // Remove current validators
                for (uint256 i = 0; i < _newValidators.length; i++) {
                    validators[_newValidators[i]] = false;
                }
        
                // Add new validators
                for (uint256 i = 0; i < _newValidators.length; i++) {
                    validators[_newValidators[i]] = true;
                    emit ValidatorAdded(_newValidators[i]);
                }
            }
        }
         */
        let code = hex::decode("00070000000000020000008003000039000000400030043f000000000301001900000060033002700000006e033001970000000102200190000000540000c13d000000040230008c000000b50000413d000000000201043b000000e002200270000000710420009c000000610000213d000000750420009c000000830000613d000000760420009c000000ab0000613d000000770220009c000000b50000c13d0000000002000416000000000202004b000000b50000c13d000000040230008a000000200220008c000000b50000413d0000000401100370000000000401043b000000780140009c000000b50000213d000000000100041a00000078011001970000000002000411000000000112004b000000d40000c13d00000000004004350000000101000039000600000001001d000000200010043f0000006e0100004100000000020004140000006e0320009c0000000002018019000000c0012002100000007a011001c70000801002000039000700000004001d01b301ae0000040f00000007030000290000000102200190000000b50000613d000000000101043b000000000101041a000000ff011001900000014c0000c13d00000000003004350000000601000029000000200010043f0000006e0300004100000000010004140000006e0210009c0000000001038019000000c0011002100000007a011001c7000080100200003901b301ae0000040f00000007050000290000000102200190000000b50000613d000000000101043b000000000201041a000001000300008a000000000232016f00000001022001bf000000000021041b00000000010004140000006e0210009c0000006e01008041000000c0011002100000007e011001c70000800d0200003900000002030000390000007f04000041000001790000013d0000000001000416000000000101004b000000b50000c13d000000000100041a0000006f011001970000000002000411000000000121019f000000000010041b0000002001000039000001000010044300000120000004430000007001000041000001b40001042e000000720420009c0000007b0000613d000000730420009c0000007b0000613d000000740220009c000000b50000c13d0000000002000416000000000202004b000000b50000c13d000000040230008a000000200220008c000000b50000413d0000000401100370000000000101043b000000780210009c000000b50000213d00000000001004350000000101000039000000200010043f000000000100001901b301970000040f000000000101041a000000ff011001900000000001000019000000010100c039000000800000013d0000000001000416000000000101004b000000b50000c13d000000000100041a0000007801100197000000800010043f0000007901000041000001b40001042e0000000002000416000000000202004b000000b50000c13d000000040230008a000000200220008c000000b50000413d0000000402100370000000000202043b000000850420009c000000b50000213d00000023042000390000008605000041000000000634004b000000000600001900000000060580190000008604400197000000000704004b0000000005008019000000860440009c000000000506c019000000000405004b000000b50000c13d0000000404200039000000000441034f000000000504043b000000870450009c000000a50000813d0000000504500210000000bf06400039000000200700008a000000000676016f0000008807600041000000890770009c000000e00000813d0000008b0100004100000000001004350000004101000039000000040010043f0000008c01000041000001b5000104300000000002000416000000000202004b000000b50000c13d000000040230008a000000200220008c000000b50000413d0000000401100370000000000401043b000000780140009c000000b70000a13d0000000001000019000001b500010430000000000100041a00000078011001970000000002000411000000000112004b000000d40000c13d00000000004004350000000101000039000600000001001d000000200010043f0000006e0100004100000000020004140000006e0320009c0000000002018019000000c0012002100000007a011001c70000801002000039000700000004001d01b301ae0000040f00000007030000290000000102200190000000b50000613d000000000101043b000000000101041a000000ff011001900000015e0000c13d000000400100043d000000440210003900000084030000410000014f0000013d0000007c01000041000000800010043f0000002001000039000000840010043f0000002101000039000000a40010043f0000008001000041000000c40010043f0000008101000041000000e40010043f0000008201000041000001b500010430000000400060043f000000800050043f00000024022000390000000004240019000000000334004b000000b50000213d000000000305004b000000f10000613d000000a003000039000000000521034f000000000505043b000000780650009c000000b50000213d00000000035304360000002002200039000000000542004b000000e90000413d000000000100041a00000078011001970000000002000411000000000112004b0000017e0000c13d000000800100043d000000000101004b0000017c0000613d000600010000003d000480100000003d00050100000000920000000003000019000700000003001d0000000501300210000000a0011000390000000001010433000000780110019700000000001004350000000601000029000000200010043f00000000010004140000006e0210009c0000006e01008041000000c0011002100000007a011001c7000000040200002901b301ae0000040f0000000102200190000000b50000613d000000000101043b000000000201041a000000050220017f000000000021041b00000007030000290000000103300039000000800100043d000000000213004b000000fd0000413d000000000101004b0000017c0000613d000380100000003d0002800d0000003d000100020000003d0000000002000019000700000002001d0000000501200210000000a001100039000400000001001d0000000001010433000000780110019700000000001004350000000601000029000000200010043f00000000010004140000006e0210009c0000006e01008041000000c0011002100000007a011001c7000000030200002901b301ae0000040f00000007040000290000000102200190000000b50000613d000000000101043b000000000201041a000000050220017f00000001022001bf000000000021041b000000800100043d000000000141004b000001930000a13d0000000401000029000000000201043300000000010004140000006e0310009c0000006e01008041000000c0011002100000007e011001c70000007805200197000000020200002900000001030000290000007f0400004101b301a90000040f00000001012001900000000702000029000000b50000613d0000000102200039000000800100043d000000000112004b0000011d0000413d0000017c0000013d000000400100043d00000044021000390000007b0300004100000000003204350000002402100039000000180300003900000000003204350000007c0200004100000000002104350000000402100039000000200300003900000000003204350000006e020000410000006e0310009c000000000102801900000040011002100000007d011001c7000001b50001043000000000003004350000000601000029000000200010043f0000006e0300004100000000010004140000006e0210009c0000000001038019000000c0011002100000007a011001c7000080100200003901b301ae0000040f00000007050000290000000102200190000000b50000613d000000000101043b000000000201041a000001000300008a000000000232016f000000000021041b00000000010004140000006e0210009c0000006e01008041000000c0011002100000007e011001c70000800d020000390000000203000039000000830400004101b301a90000040f0000000101200190000000b50000613d0000000001000019000001b40001042e000000400100043d0000006402100039000000810300004100000000003204350000004402100039000000800300004100000000003204350000002402100039000000210300003900000000003204350000007c0200004100000000002104350000000402100039000000200300003900000000003204350000006e020000410000006e0310009c000000000102801900000040011002100000008a011001c7000001b5000104300000008b0100004100000000001004350000003201000039000000a80000013d0000006e020000410000006e0310009c000000000102801900000000030004140000006e0430009c0000000003028019000000c0023002100000004001100210000000000121019f0000007a011001c7000080100200003901b301ae0000040f0000000102200190000001a70000613d000000000101043b000000000001042d0000000001000019000001b500010430000001ac002104210000000102000039000000000001042d0000000002000019000000000001042d000001b1002104230000000102000039000000000001042d0000000002000019000000000001042d000001b300000432000001b40001042e000001b5000104300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffffffffffff0000000000000000000000000000000000000000000000020000000000000000000000000000004000000100000000000000000000000000000000000000000000000000000000000000000000000000893d20e700000000000000000000000000000000000000000000000000000000893d20e8000000000000000000000000000000000000000000000000000000008da5cb5b00000000000000000000000000000000000000000000000000000000fa52c7d80000000000000000000000000000000000000000000000000000000025bed0650000000000000000000000000000000000000000000000000000000040a141ff000000000000000000000000000000000000000000000000000000004d238c8e000000000000000000000000ffffffffffffffffffffffffffffffffffffffff0000000000000000000000000000000000000020000000800000000000000000020000000000000000000000000000000000004000000000000000000000000056616c696461746f7220616c726561647920657869737473000000000000000008c379a00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000640000000000000000000000000200000000000000000000000000000000000000000000000000000000000000e366c1c0452ed8eec96861e9e54141ebff23c9ec89fe27b996b45f5ec38849874f6e6c79206f776e65722063616e2063616c6c20746869732066756e6374696f6e000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000084000000800000000000000000e1434e25d6611e0db941968fdc97811c982ac1602e951637d206f5fdda9dd8f156616c696461746f7220646f6573206e6f742065786973740000000000000000000000000000000000000000000000000000000000000000ffffffffffffffff80000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000ffffffffffffffffffffffffffffffffffffffffffffffff0000000000000000ffffffffffffffffffffffffffffffffffffffffffffffff000000000000008000000000000000000000000000000000000000840000000000000000000000004e487b710000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000240000000000000000000000000000000000000000000000000000000000000000000000000000000000000000cd0c4feb9f0d48880f7305c8aeefe7f24f3ce4e3d444ad588af0e18a862601e6").unwrap();

        let deploy_tx = account.get_deploy_tx(&code, None, TxType::L2);
        let res = tx_sender.submit_tx(deploy_tx.tx.clone().try_into().unwrap()).await.unwrap();

        // Attempt to read from contract. 
        // `owner` field should have some value.
        // Explicit `getOwner` func was added, although it shouldn't be needed (Solidity generates it automatically).
        let block_id = BlockId::Number(BlockNumber::Pending);
        let mut conn = pool.connection(ctx).await.unwrap().0;
        let start_info = BlockStartInfo::new(&mut conn, Duration::from_secs(10)).await?;
        let block_args = zksync_node_api_server::execution_sandbox::BlockArgs::new(&mut conn, block_id, &start_info).await.unwrap();
        let call_overrides = CallOverrides { enforced_base_fee: None};
        let call_tx: L2Tx = account.get_l2_tx_for_execute(
            Execute {
                contract_address: deploy_tx.address,
                calldata: hex::decode("8da5cb5b").unwrap(), // keccak256("getOwner()") = 0x8da5cb5b
                value: Default::default(),
                factory_deps: None,
            },
            Some(fee(1_000_000)),
        ).try_into().unwrap();
        let res = tx_sender.eth_call(block_args, call_overrides, call_tx.clone()).await.unwrap();
        // output the result.
        panic!("{:?}", res);

        Ok(())
    })
        .await
        .unwrap();
}

// In the current implementation, consensus certificates are created asynchronously
// for the L2 blocks constructed by the StateKeeper. This means that consensus actor
// is effectively just back filling the consensus certificates for the L2 blocks in storage.
#[test_casing(2, [false, true])]
#[tokio::test(flavor = "multi_thread")]
async fn test_validator(from_snapshot: bool) {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::AffineClock::new(10.));
    let rng = &mut ctx.rng();
    let setup = Setup::new(rng, 1);
    let cfgs = new_configs(rng, &setup, 0);

    scope::run!(ctx, |ctx, s| async {
        tracing::info!("Start state keeper.");
        let pool = new_pool(from_snapshot).await;
        let (mut sk, runner) = testonly::StateKeeper::new(ctx, pool.clone()).await?;
        s.spawn_bg(runner.run(ctx));

        tracing::info!("Populate storage with a bunch of blocks.");
        sk.push_random_blocks(rng, 5).await;
        pool
            .wait_for_payload(ctx, sk.last_block())
            .await
            .context("sk.wait_for_payload(<1st phase>)")?;

        tracing::info!("Restart consensus actor a couple times, making it process a bunch of blocks each time.");
        for iteration in 0..3 {
            tracing::info!("iteration {iteration}");
            scope::run!(ctx, |ctx, s| async {
                tracing::info!("Start consensus actor");
                // In the first iteration it will initialize genesis.
                let (cfg,secrets) = testonly::config(&cfgs[0]);
                s.spawn_bg(run_main_node(ctx, cfg, secrets, pool.clone()));

                tracing::info!("Generate couple more blocks and wait for consensus to catch up.");
                sk.push_random_blocks(rng, 3).await;
                pool
                    .wait_for_certificate(ctx, sk.last_block())
                    .await
                    .context("wait_for_certificate(<2nd phase>)")?;

                tracing::info!("Synchronously produce blocks one by one, and wait for consensus.");
                for _ in 0..2 {
                    sk.push_random_blocks(rng, 1).await;
                    pool
                        .wait_for_certificate(ctx, sk.last_block())
                        .await
                        .context("wait_for_certificate(<3rd phase>)")?;
                }

                tracing::info!("Verify all certificates");
                pool
                    .wait_for_certificates_and_verify(ctx, sk.last_block())
                    .await
                    .context("wait_for_certificates_and_verify()")?;
                Ok(())
            })
            .await
            .context(iteration)?;
        }
        Ok(())
    })
        .await
        .unwrap();
}

// Test running a validator node and 2 full nodes recovered from different snapshots.
#[tokio::test(flavor = "multi_thread")]
async fn test_nodes_from_various_snapshots() {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::AffineClock::new(10.));
    let rng = &mut ctx.rng();
    let setup = Setup::new(rng, 1);
    let validator_cfg = new_configs(rng, &setup, 0).pop().unwrap();

    scope::run!(ctx, |ctx, s| async {
        tracing::info!("spawn validator");
        let validator_pool = ConnectionPool::from_genesis().await;
        let (mut validator, runner) =
            testonly::StateKeeper::new(ctx, validator_pool.clone()).await?;
        s.spawn_bg(runner.run(ctx).instrument(tracing::info_span!("validator")));
        let (cfg, secrets) = testonly::config(&validator_cfg);
        s.spawn_bg(run_main_node(ctx, cfg, secrets, validator_pool.clone()));

        tracing::info!("produce some batches");
        validator.push_random_blocks(rng, 5).await;
        validator.seal_batch().await;
        validator_pool
            .wait_for_certificate(ctx, validator.last_block())
            .await?;

        tracing::info!("take snapshot and start a node from it");
        let snapshot = validator_pool.snapshot(ctx).await?;
        let node_pool = ConnectionPool::from_snapshot(snapshot).await;
        let (node, runner) = testonly::StateKeeper::new(ctx, node_pool.clone()).await?;
        s.spawn_bg(runner.run(ctx).instrument(tracing::info_span!("node1")));
        let conn = validator.connect(ctx).await?;
        s.spawn_bg(async {
            let cfg = new_fullnode(&mut ctx.rng(), &validator_cfg);
            node.run_consensus(ctx, conn, &cfg).await
        });

        tracing::info!("produce more batches");
        validator.push_random_blocks(rng, 5).await;
        validator.seal_batch().await;
        node_pool
            .wait_for_certificate(ctx, validator.last_block())
            .await?;

        tracing::info!("take another snapshot and start a node from it");
        let snapshot = validator_pool.snapshot(ctx).await?;
        let node_pool2 = ConnectionPool::from_snapshot(snapshot).await;
        let (node, runner) = testonly::StateKeeper::new(ctx, node_pool2.clone()).await?;
        s.spawn_bg(runner.run(ctx).instrument(tracing::info_span!("node2")));
        let conn = validator.connect(ctx).await?;
        s.spawn_bg(async {
            let cfg = new_fullnode(&mut ctx.rng(), &validator_cfg);
            node.run_consensus(ctx, conn, &cfg).await
        });

        tracing::info!("produce more blocks and compare storages");
        validator.push_random_blocks(rng, 5).await;
        let want = validator_pool
            .wait_for_certificates_and_verify(ctx, validator.last_block())
            .await?;
        // node stores should be suffixes for validator store.
        for got in [
            node_pool
                .wait_for_certificates_and_verify(ctx, validator.last_block())
                .await?,
            node_pool2
                .wait_for_certificates_and_verify(ctx, validator.last_block())
                .await?,
        ] {
            assert_eq!(want[want.len() - got.len()..], got[..]);
        }
        Ok(())
    })
    .await
    .unwrap();
}

// Test running a validator node and a couple of full nodes.
// Validator is producing signed blocks and fetchers are expected to fetch
// them directly or indirectly.
#[test_casing(2, [false, true])]
#[tokio::test(flavor = "multi_thread")]
async fn test_full_nodes(from_snapshot: bool) {
    const NODES: usize = 2;

    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::AffineClock::new(10.));
    let rng = &mut ctx.rng();
    let setup = Setup::new(rng, 1);
    let validator_cfgs = new_configs(rng, &setup, 0);

    // topology:
    // validator <-> node <-> node <-> ...
    let mut node_cfgs = vec![];
    for _ in 0..NODES {
        node_cfgs.push(new_fullnode(
            rng,
            node_cfgs.last().unwrap_or(&validator_cfgs[0]),
        ));
    }

    // Run validator and fetchers in parallel.
    scope::run!(ctx, |ctx, s| async {
        let validator_pool = new_pool(from_snapshot).await;
        let (mut validator, runner) =
            testonly::StateKeeper::new(ctx, validator_pool.clone()).await?;
        s.spawn_bg(async {
            runner
                .run(ctx)
                .instrument(tracing::info_span!("validator"))
                .await
                .context("validator")
        });
        tracing::info!("Generate a couple of blocks, before initializing consensus genesis.");
        validator.push_random_blocks(rng, 5).await;
        // API server needs at least 1 L1 batch to start.
        validator.seal_batch().await;
        validator_pool
            .wait_for_payload(ctx, validator.last_block())
            .await
            .unwrap();

        tracing::info!("Run validator.");
        let (cfg, secrets) = testonly::config(&validator_cfgs[0]);
        s.spawn_bg(run_main_node(ctx, cfg, secrets, validator_pool.clone()));

        tracing::info!("Run nodes.");
        let mut node_pools = vec![];
        for (i, cfg) in node_cfgs.iter().enumerate() {
            let i = ctx::NoCopy(i);
            let pool = new_pool(from_snapshot).await;
            let (node, runner) = testonly::StateKeeper::new(ctx, pool.clone()).await?;
            node_pools.push(pool.clone());
            s.spawn_bg(async {
                let i = i;
                runner
                    .run(ctx)
                    .instrument(tracing::info_span!("node", i = *i))
                    .await
                    .with_context(|| format!("node{}", *i))
            });
            s.spawn_bg(node.run_consensus(ctx, validator.connect(ctx).await?, cfg));
        }

        tracing::info!("Make validator produce blocks and wait for fetchers to get them.");
        // Note that block from before and after genesis have to be fetched.
        validator.push_random_blocks(rng, 5).await;
        let want_last = validator.last_block();
        let want = validator_pool
            .wait_for_certificates_and_verify(ctx, want_last)
            .await?;
        for pool in &node_pools {
            assert_eq!(
                want,
                pool.wait_for_certificates_and_verify(ctx, want_last)
                    .await?
            );
        }
        Ok(())
    })
    .await
    .unwrap();
}

// Test running external node (non-leader) validators.
#[test_casing(2, [false, true])]
#[tokio::test(flavor = "multi_thread")]
async fn test_en_validators(from_snapshot: bool) {
    const NODES: usize = 3;

    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::AffineClock::new(10.));
    let rng = &mut ctx.rng();
    let setup = Setup::new(rng, NODES);
    let cfgs = new_configs(rng, &setup, 1);

    // Run all nodes in parallel.
    scope::run!(ctx, |ctx, s| async {
        let main_node_pool = new_pool(from_snapshot).await;
        let (mut main_node, runner) =
            testonly::StateKeeper::new(ctx, main_node_pool.clone()).await?;
        s.spawn_bg(async {
            runner
                .run(ctx)
                .instrument(tracing::info_span!("main_node"))
                .await
                .context("main_node")
        });
        tracing::info!("Generate a couple of blocks, before initializing consensus genesis.");
        main_node.push_random_blocks(rng, 5).await;
        // API server needs at least 1 L1 batch to start.
        main_node.seal_batch().await;
        main_node_pool
            .wait_for_payload(ctx, main_node.last_block())
            .await
            .unwrap();

        tracing::info!("wait until the API server is actually available");
        // as otherwise waiting for view synchronization will take a while.
        main_node.connect(ctx).await?;

        tracing::info!("Run main node with all nodes being validators.");
        let (mut cfg, secrets) = testonly::config(&cfgs[0]);
        cfg.genesis_spec.as_mut().unwrap().validators = setup
            .keys
            .iter()
            .map(|k| WeightedValidator {
                key: ValidatorPublicKey(k.public().encode()),
                weight: 1,
            })
            .collect();
        s.spawn_bg(run_main_node(ctx, cfg, secrets, main_node_pool.clone()));

        tracing::info!("Run external nodes.");
        let mut ext_node_pools = vec![];
        for (i, cfg) in cfgs[1..].iter().enumerate() {
            let i = ctx::NoCopy(i);
            let pool = new_pool(from_snapshot).await;
            let (ext_node, runner) = testonly::StateKeeper::new(ctx, pool.clone()).await?;
            ext_node_pools.push(pool.clone());
            s.spawn_bg(async {
                let i = i;
                runner
                    .run(ctx)
                    .instrument(tracing::info_span!("en", i = *i))
                    .await
                    .with_context(|| format!("en{}", *i))
            });
            s.spawn_bg(ext_node.run_consensus(ctx, main_node.connect(ctx).await?, cfg));
        }

        tracing::info!("Make the main node produce blocks and wait for consensus to finalize them");
        main_node.push_random_blocks(rng, 5).await;
        let want_last = main_node.last_block();
        let want = main_node_pool
            .wait_for_certificates_and_verify(ctx, want_last)
            .await?;
        for pool in &ext_node_pools {
            assert_eq!(
                want,
                pool.wait_for_certificates_and_verify(ctx, want_last)
                    .await?
            );
        }
        Ok(())
    })
    .await
    .unwrap();
}

// Test fetcher back filling missing certs.
#[test_casing(2, [false, true])]
#[tokio::test(flavor = "multi_thread")]
async fn test_p2p_fetcher_backfill_certs(from_snapshot: bool) {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::AffineClock::new(10.));
    let rng = &mut ctx.rng();
    let setup = Setup::new(rng, 1);
    let validator_cfg = new_configs(rng, &setup, 0)[0].clone();
    let node_cfg = new_fullnode(rng, &validator_cfg);

    scope::run!(ctx, |ctx, s| async {
        tracing::info!("Spawn validator.");
        let validator_pool = new_pool(from_snapshot).await;
        let (mut validator, runner) =
            testonly::StateKeeper::new(ctx, validator_pool.clone()).await?;
        s.spawn_bg(runner.run(ctx));
        let (cfg, secrets) = testonly::config(&validator_cfg);
        s.spawn_bg(run_main_node(ctx, cfg, secrets, validator_pool.clone()));
        // API server needs at least 1 L1 batch to start.
        validator.seal_batch().await;
        let client = validator.connect(ctx).await?;

        let node_pool = new_pool(from_snapshot).await;

        tracing::info!("Run p2p fetcher.");
        scope::run!(ctx, |ctx, s| async {
            let (node, runner) = testonly::StateKeeper::new(ctx, node_pool.clone()).await?;
            s.spawn_bg(runner.run(ctx));
            s.spawn_bg(node.run_consensus(ctx, client.clone(), &node_cfg));
            validator.push_random_blocks(rng, 3).await;
            node_pool
                .wait_for_certificate(ctx, validator.last_block())
                .await?;
            Ok(())
        })
        .await
        .unwrap();

        tracing::info!("Run centralized fetcher.");
        scope::run!(ctx, |ctx, s| async {
            let (node, runner) = testonly::StateKeeper::new(ctx, node_pool.clone()).await?;
            s.spawn_bg(runner.run(ctx));
            s.spawn_bg(node.run_fetcher(ctx, client.clone()));
            validator.push_random_blocks(rng, 3).await;
            node_pool
                .wait_for_payload(ctx, validator.last_block())
                .await?;
            Ok(())
        })
        .await
        .unwrap();

        tracing::info!("Run p2p fetcher again.");
        scope::run!(ctx, |ctx, s| async {
            let (node, runner) = testonly::StateKeeper::new(ctx, node_pool.clone()).await?;
            s.spawn_bg(runner.run(ctx));
            s.spawn_bg(node.run_consensus(ctx, client.clone(), &node_cfg));
            validator.push_random_blocks(rng, 3).await;
            let want = validator_pool
                .wait_for_certificates_and_verify(ctx, validator.last_block())
                .await?;
            let got = node_pool
                .wait_for_certificates_and_verify(ctx, validator.last_block())
                .await?;
            assert_eq!(want, got);
            Ok(())
        })
        .await
        .unwrap();
        Ok(())
    })
    .await
    .unwrap();
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn test_centralized_fetcher(from_snapshot: bool) {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();

    scope::run!(ctx, |ctx, s| async {
        tracing::info!("Spawn a validator.");
        let validator_pool = new_pool(from_snapshot).await;
        let (mut validator, runner) =
            testonly::StateKeeper::new(ctx, validator_pool.clone()).await?;
        s.spawn_bg(runner.run(ctx).instrument(tracing::info_span!("validator")));

        tracing::info!("Produce a batch (to make api server start)");
        // TODO: ensure at least L1 batch in `testonly::StateKeeper::new()` to make it fool proof.
        validator.seal_batch().await;

        tracing::info!("Spawn a node.");
        let node_pool = new_pool(from_snapshot).await;
        let (node, runner) = testonly::StateKeeper::new(ctx, node_pool.clone()).await?;
        s.spawn_bg(runner.run(ctx).instrument(tracing::info_span!("fetcher")));
        s.spawn_bg(node.run_fetcher(ctx, validator.connect(ctx).await?));

        tracing::info!("Produce some blocks and wait for node to fetch them");
        validator.push_random_blocks(rng, 10).await;
        let want = validator_pool
            .wait_for_payload(ctx, validator.last_block())
            .await?;
        let got = node_pool
            .wait_for_payload(ctx, validator.last_block())
            .await?;
        assert_eq!(want, got);
        Ok(())
    })
    .await
    .unwrap();
}

/// Tests that generated L1 batch witnesses can be verified successfully.
/// TODO: add tests for verification failures.
#[tokio::test]
async fn test_batch_witness() {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();

    scope::run!(ctx, |ctx, s| async {
        let pool = ConnectionPool::from_genesis().await;
        let (mut node, runner) = testonly::StateKeeper::new(ctx, pool.clone()).await?;
        s.spawn_bg(runner.run_real(ctx));

        tracing::info!("analyzing storage");
        {
            let mut conn = pool.connection(ctx).await.unwrap();
            let mut n = validator::BlockNumber(0);
            while let Some(p) = conn.payload(ctx, n).await? {
                tracing::info!("block[{n}] = {p:?}");
                n = n + 1;
            }
        }

        // Seal a bunch of batches.
        node.push_random_blocks(rng, 10).await;
        node.seal_batch().await;
        pool.wait_for_batch(ctx, node.last_sealed_batch()).await?;
        // We can verify only 2nd batch onward, because
        // batch witness verifies parent of the last block of the
        // previous batch (and 0th batch contains only 1 block).
        for n in 2..=node.last_sealed_batch().0 {
            let n = L1BatchNumber(n);
            let batch_with_witness = node.load_batch_with_witness(ctx, n).await?;
            let commit = node.load_batch_commit(ctx, n).await?;
            batch_with_witness.verify(&commit)?;
        }
        Ok(())
    })
    .await
    .unwrap();
}

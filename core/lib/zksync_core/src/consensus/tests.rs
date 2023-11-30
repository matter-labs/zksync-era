struct Acc {
    next_batch: L1BatchNumber,
    next_block: MiniblockNumber, 
    next_timestamp: u64,

    fee_per_gas: u64,
    gas_per_pubdata: u32,

    actions: Vec<SyncAction>,
    batch_executor: TestBatchExecutorBuilder,
}

async fn run_metadata_calculator(ctx: &ctx::Ctx, pool: ConnectionPool) -> anyhow::Result<()> {
    const POLL_INTERVAL: time::Duration = time::Duration::from_millis(50);
    let mut n = L1BatchNumber(0);
    while let Ok(()) = ctx.sleep(POLL_INTERVAL).await {
        let mut storage = pool.access_storage().await?;
        let last = storage.blocks_dal().get_sealed_l1_batch_number().await?;
        while n < last {
            let metadata = create_l1_batch_metadata(n);
            storage.blocks_dal().save_l1_batch_metadata(n, &metadata, H256::zero(), false).await?;
            n += 1;
        }
    }
    Ok(())
}

impl Acc {
    pub fn open_batch(&mut self) {
        self.actions.push(SyncAction::OpenBatch {
            number: self.next_batch,
            timestamp: self.next_timestamp,
            l1_gas_price: 2,
            l2_fair_gas_price: 3,
            operator_address: Default::default(),
            protocol_version: ProtocolVersionId::latest(),
            first_miniblock_info: (self.next_block, 1),
            prev_miniblock_hash: H256::default(),
        });
        self.next_batch += 1;
        self.next_block += 1;
        self.next_timestamp += 1;
    }

    pub fn transaction(&mut self, fee_per_gas: u64, gas_per_pubdata: u32) -> L2Tx {
        self.actions.push(SyncAction::Tx(Box::new(
            create_l2_transaction(self.fee_per_gas, self.gas_per_pub_data).into(),
        )));
    }

    pub fn seal_block(&mut self) {
        self.actions.push(SyncAction::SealMiniblock(None));
    }

    pub fn fictive_miniblock(&mut self) {
        self.actions.push(SyncAction::Miniblock {
            number: self.next_block,
            timestamp: self.next_timestamp,
            virtual_blocks: 0,
        });
        self.next_block += 1;
        self.next_timestamp += 1;
    }
    pub fn seal_batch(&mut self) {
        self.actions.push(SyncCaction::SealBatch {
            virtual_blocks: 0,
            consensus: None,
        });
        self.batch_executor.push_successful_transactions(/*TODO: txs*/);
    }

    pub async fn run(self, ctx: &ctx::Ctx, pool: ConnectionPool) -> anyhow::Result<()> { 
        scope::run!(ctx, |ctx, s| {
            // ensure genesis
            if storage.blocks_dal().is_genesis_needed().await.unwrap() {
                ensure_genesis_state(storage, L2ChainId::default(), &GenesisParams::mock()).await?;
            }
            let (actions_sender, action_queue) = ActionQueue::new();
            actions_sender.push_actions(self.actions).await;
            let sync_state = SyncState::new();
            let io = ExternalIO::new(
                pool,
                actions_queue,
                sync_state.clone(),
                MockMainNodeClient::default().into(),
                Address::repeat_byte(1),
                u32::MAX,
                L2ChainId::default(),
            )
            .await;
            let (stop_sender, stop_receiver) = watch::channel(false);
            s.spawn_bg(run_metadata_calculator(ctx, pool.clone())); 
            s.spawn_bg(async { ctx.canceled().await; stop_sender.send(true); Ok(()) });
            s.spawn_bg(ZkSyncStateKeeper::without_sealer(stop_receiver,io.into(),self.batch_executor.into()).run());
            while sync_state.get_local_block() < self.next_block-1 {
                ctx.sleep(POLL_INTERVAL).await?;
            }
            Ok(())
        }).await
    }
}

/// Returns tx hashes of all generated transactions, grouped by the L1 batch.
pub(super) async fn run_state_keeper_with_multiple_l1_batches(pool: ConnectionPool) {
    let l1_batch = open_l1_batch(1, 1, 1);
    let first_tx = create_l2_transaction(10, 100);
    let first_tx_hash = first_tx.hash();
    let first_tx = SyncAction::Tx(Box::new(first_tx.into()));
    let first_l1_batch_actions = vec![l1_batch, first_tx, SyncAction::SealMiniblock(None)];

    let fictive_miniblock = SyncAction::Miniblock {
        number: MiniblockNumber(2),
        timestamp: 2,
        virtual_blocks: 0,
    };
    let seal_l1_batch = SyncAction::SealBatch {
        virtual_blocks: 0,
        consensus: None,
    };
    let fictive_miniblock_actions = vec![fictive_miniblock, seal_l1_batch];

    let l1_batch = open_l1_batch(2, 3, 3);
    let second_tx = create_l2_transaction(10, 100);
    let second_tx_hash = second_tx.hash();
    let second_tx = SyncAction::Tx(Box::new(second_tx.into()));
    let second_l1_batch_actions = vec![l1_batch, second_tx, SyncAction::SealMiniblock(None)];

    let (actions_sender, action_queue) = ActionQueue::new();
    let state_keeper = StateKeeperHandles::new(
        pool.clone(),
        action_queue,
        &[&[first_tx_hash], &[second_tx_hash]],
    )
    .await;
    actions_sender.push_actions(first_l1_batch_actions).await;
    actions_sender.push_actions(fictive_miniblock_actions).await;
    actions_sender.push_actions(second_l1_batch_actions).await;

    let hash_task = tokio::spawn(mock_l1_batch_hash_computation(pool.clone(), 1));
    // Wait until the miniblocks are sealed.
    state_keeper
        .wait(|state| state.get_local_block() == MiniblockNumber(3))
        .await;
    hash_task.await.unwrap();

    vec![vec![first_tx_hash], vec![second_tx_hash]]
}

//////////////////////////////////////////////////////////////

#[tokio::test]
async fn syncing_via_gossip_fetcher_with_multiple_l1_batches() {
    let initial_block_count = 5;
    zksync_concurrency::testonly::abort_on_panic();

    let pool = ConnectionPool::test_pool().await;
    let tx_hashes = run_state_keeper_with_multiple_l1_batches(pool.clone()).await;
    let tx_hashes: Vec<_> = tx_hashes.iter().map(Vec::as_slice).collect();

    let mut storage = pool.access_storage().await.unwrap();
    let genesis_block_payload = block_payload(&mut storage, 0).await;
    let ctx = &ctx::test_root(&ctx::AffineClock::new(CLOCK_SPEEDUP as f64));
    let rng = &mut ctx.rng();
    let mut validator = FullValidatorConfig::for_single_validator(rng, genesis_block_payload);
    let validator_set = validator.node_config.validators.clone();
    let external_node = validator.connect_full_node(rng);

    let (genesis_block, blocks) =
        get_blocks_and_reset_storage(storage, &validator.validator_key).await;
    assert_eq!(blocks.len(), 3); // 2 real + 1 fictive blocks
    tracing::trace!("Node storage reset");
    let (initial_blocks, delayed_blocks) = blocks.split_at(initial_block_count);

    let validator_storage = Arc::new(InMemoryStorage::new(genesis_block));
    for block in initial_blocks {
        validator_storage.put_block(ctx, block).await.unwrap();
    }
    let validator = Executor::new(
        validator.node_config,
        validator.node_key,
        validator_storage.clone(),
    )
    .unwrap();

    let (actions_sender, actions) = ActionQueue::new();
    let state_keeper = StateKeeperHandles::new(pool.clone(), actions, &tx_hashes).await;
    scope::run!(ctx, |ctx, s| async {
        s.spawn_bg(validator.run(ctx));
        s.spawn_bg(async {
            for block in delayed_blocks {
                ctx.sleep(POLL_INTERVAL).await?;
                validator_storage.put_block(ctx, block).await?;
            }
            Ok(())
        });

        let cloned_pool = pool.clone();
        s.spawn_bg(async {
            mock_l1_batch_hash_computation(cloned_pool, 1).await;
            Ok(())
        });
        s.spawn_bg(run_gossip_fetcher_inner(
            ctx,
            pool.clone(),
            actions_sender,
            external_node.node_config,
            external_node.node_key,
        ));

        state_keeper
            .wait(|state| state.get_local_block() == MiniblockNumber(3))
            .await;
        Ok(())
    })
    .await
    .unwrap();

    // Check that received blocks have consensus fields persisted.
    let mut storage = pool.access_storage().await.unwrap();
    for number in [1, 2, 3] {
        let block = load_final_block(&mut storage, number).await;
        block.justification.verify(&validator_set, 1).unwrap();
    }
}

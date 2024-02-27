#[tokio::test]
async fn fetcher_basics() {
    abort_on_panic();
    set_timeout(TEST_TIMEOUT);
    let ctx = &ctx::test_root(&ctx::RealClock);

    scope::run!(ctx,|ctx,s| async {
        let pool = ConnectionPool::test_pool().await;
        ensure_genesis(&mut storage).await;
        let mut mock_client = MockMainNodeClient::default();
        mock_client.push_l1_batch(0);
        // ^ The genesis L1 batch will not be queried, so we're OK with filling it with non-authentic data
        let mut tx_hashes = VecDeque::from(mock_client.push_l1_batch(1));
        tx_hashes.extend(mock_client.push_l1_batch(2));

        let (actions_sender, mut actions) = ActionQueue::new();
        let sync_state = SyncState::default();
        let fetcher = RpcFetcher {
            sync_state: sync_state.clone(),
            client: Box::new(mock_client),
        };
        s.spawn_bg(fetcher.run(ctx,Store(pool.clone()),actions_sender));

        // Check that `sync_state` is updated.
        sync::wait_for(ctx, &mut sync_state.subscribe(), |s|s.main_node_block()>MiniblockNumber(5)).await.unwrap();

        // Check generated actions. Some basic checks are performed by `ActionQueueSender`.
        let mut current_l1_batch_number = L1BatchNumber(0);
        let mut current_miniblock_number = MiniblockNumber(0);
        let mut tx_count_in_miniblock = 0;
        loop {
            match actions.recv_action() {
                SyncAction::OpenBatch { number, .. } => {
                    current_l1_batch_number += 1;
                    current_miniblock_number += 1; // First miniblock is implicitly opened
                    tx_count_in_miniblock = 0;
                    assert_eq!(number, current_l1_batch_number);
                }
                SyncAction::Miniblock { number, .. } => {
                    current_miniblock_number += 1;
                    tx_count_in_miniblock = 0;
                    assert_eq!(number, current_miniblock_number);
                }
                SyncAction::SealBatch { virtual_blocks, .. } => {
                    assert_eq!(virtual_blocks, 0);
                    assert_eq!(tx_count_in_miniblock, 0);
                    if current_miniblock_number == MiniblockNumber(5) {
                        break;
                    }
                }
                SyncAction::Tx(tx) => {
                    assert_eq!(tx.hash(), tx_hashes.pop_front().unwrap());
                    tx_count_in_miniblock += 1;
                }
                SyncAction::SealMiniblock => {
                    assert_eq!(tx_count_in_miniblock, 1);
                }
            }
        }
    }).await.unwrap();
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn fetcher_with_real_server(snapshot_recovery: bool) {
    let pool = ConnectionPool::test_pool().await;
    // Fill in transactions grouped in multiple L1 batches in the storage. We need at least one L1 batch,
    // so that the API server doesn't hang up waiting for it.
    let (snapshot, tx_hashes) =
        run_state_keeper_with_multiple_l1_batches(pool.clone(), snapshot_recovery).await;
    let mut tx_hashes: VecDeque<_> = tx_hashes.into_iter().flatten().collect();

    // Start the API server.
    let network_config = NetworkConfig::for_tests();
    let contracts_config = ContractsConfig::for_tests();
    let web3_config = Web3JsonRpcConfig::for_tests();
    let api_config = InternalApiConfig::new(&network_config, &web3_config, &contracts_config);
    let (stop_sender, stop_receiver) = watch::channel(false);
    let mut server_handles = spawn_http_server(
        api_config,
        pool.clone(),
        Default::default(),
        stop_receiver.clone(),
    )
    .await;
    let server_addr = &server_handles.wait_until_ready().await;
    s.spawn_bg(async {
        ctx.canceled().await;
        stop_sender.send_replace(true);
        server_handles.shutdown().await;
    });

    // Start the fetcher connected to the API server.
    let sync_state = SyncState::default();
    let (actions_sender, mut actions) = ActionQueue::new();
    let client = <dyn MainNodeClient>::json_rpc(&format!("http://{server_addr}/")).unwrap();
    let fetcher = MainNodeFetcher {
        client: CachingMainNodeClient::new(Box::new(client)),
        cursor: IoCursor {
            next_miniblock: snapshot.miniblock_number + 1,
            prev_miniblock_hash: snapshot.miniblock_hash,
            prev_miniblock_timestamp: snapshot.miniblock_timestamp,
            l1_batch: snapshot.l1_batch_number,
        },
        actions: actions_sender,
        sync_state: sync_state.clone(),
        stop_receiver,
    };
    let fetcher_task = tokio::spawn(fetcher.run());

    // Check generated actions.
    let mut current_miniblock_number = snapshot.miniblock_number;
    let mut current_l1_batch_number = snapshot.l1_batch_number + 1;
    let mut tx_count_in_miniblock = 0;
    let miniblock_number_to_tx_count = HashMap::from([
        (snapshot.miniblock_number + 1, 1),
        (snapshot.miniblock_number + 2, 0),
        (snapshot.miniblock_number + 3, 1),
    ]);
    let started_at = Instant::now();
    let deadline = started_at + TEST_TIMEOUT;
    loop {
        let action = tokio::time::timeout_at(deadline.into(), actions.recv_action())
            .await
            .unwrap();
        match action {
            SyncAction::OpenBatch {
                number,
                first_miniblock_info,
                ..
            } => {
                assert_eq!(number, current_l1_batch_number);
                current_miniblock_number += 1; // First miniblock is implicitly opened
                tx_count_in_miniblock = 0;
                assert_eq!(first_miniblock_info.0, current_miniblock_number);
            }
            SyncAction::SealBatch { .. } => {
                current_l1_batch_number += 1;
            }
            SyncAction::Miniblock { number, .. } => {
                current_miniblock_number += 1;
                tx_count_in_miniblock = 0;
                assert_eq!(number, current_miniblock_number);
            }
            SyncAction::Tx(tx) => {
                assert_eq!(tx.hash(), tx_hashes.pop_front().unwrap());
                tx_count_in_miniblock += 1;
            }
            SyncAction::SealMiniblock => {
                assert_eq!(
                    tx_count_in_miniblock,
                    miniblock_number_to_tx_count[&current_miniblock_number]
                );
                if current_miniblock_number == snapshot.miniblock_number + 3 {
                    break;
                }
            }
        }
    }
}

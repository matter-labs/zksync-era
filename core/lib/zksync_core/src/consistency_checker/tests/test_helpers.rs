use std::{collections::HashMap, slice};

use tokio::sync::mpsc;
use zksync_eth_client::clients::MockEthereum;
use zksync_types::{web3::contract::Options, L2ChainId, ProtocolVersion};

use super::*;
use crate::genesis::{ensure_genesis_state, GenesisParams};

pub(crate) fn build_commit_tx_input_data_is_correct(
    l1_batch_commit_data_generator: Arc<dyn L1BatchCommitDataGenerator>,
) {
    let contract = zksync_contracts::zksync_contract();
    let commit_function = contract.function("commitBatches").unwrap();
    let batches = vec![
        create_l1_batch_with_metadata(1),
        create_l1_batch_with_metadata(2),
    ];

    let commit_tx_input_data =
        build_commit_tx_input_data(&batches, l1_batch_commit_data_generator.clone());

    for batch in &batches {
        let commit_data = ConsistencyChecker::extract_commit_data(
            &commit_tx_input_data,
            commit_function,
            batch.header.number,
        )
        .unwrap();
        assert_eq!(
            commit_data,
            CommitBatchInfo::new(batch, l1_batch_commit_data_generator.clone().clone())
                .into_token()
        );
    }
}

pub(crate) async fn normal_checker_function(
    batches_per_transaction: usize,
    (mapper_name, save_actions_mapper): (&'static str, SaveActionMapper),
    l1_batch_commit_data_generator: Arc<dyn L1BatchCommitDataGenerator>,
) {
    println!("Using save_actions_mapper={mapper_name}");

    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
        .await
        .unwrap();

    let l1_batches: Vec<_> = (1..=10).map(create_l1_batch_with_metadata).collect();
    let mut commit_tx_hash_by_l1_batch = HashMap::with_capacity(l1_batches.len());
    let client = MockEthereum::default();

    for (i, l1_batches) in l1_batches.chunks(batches_per_transaction).enumerate() {
        let input_data =
            build_commit_tx_input_data(l1_batches, l1_batch_commit_data_generator.clone());
        let signed_tx = client.sign_prepared_tx(
            input_data.clone(),
            Options {
                nonce: Some(i.into()),
                ..Options::default()
            },
        );
        let signed_tx = signed_tx.unwrap();
        client.send_raw_tx(signed_tx.raw_tx).await.unwrap();
        client.execute_tx(signed_tx.hash, true, 1);

        commit_tx_hash_by_l1_batch.extend(
            l1_batches
                .iter()
                .map(|batch| (batch.header.number, signed_tx.hash)),
        );
    }

    let (l1_batch_updates_sender, mut l1_batch_updates_receiver) = mpsc::unbounded_channel();
    let checker = ConsistencyChecker {
        l1_batch_updater: Box::new(l1_batch_updates_sender),
        ..create_mock_checker(client, pool.clone(), l1_batch_commit_data_generator)
    };

    let (stop_sender, stop_receiver) = watch::channel(false);
    let checker_task = tokio::spawn(checker.run(stop_receiver));

    // Add new batches to the storage.
    for save_action in save_actions_mapper(&l1_batches) {
        save_action
            .apply(&mut storage, &commit_tx_hash_by_l1_batch)
            .await;
        tokio::time::sleep(Duration::from_millis(7)).await;
    }

    // Wait until all batches are checked.
    loop {
        let checked_batch = l1_batch_updates_receiver.recv().await.unwrap();
        if checked_batch == l1_batches.last().unwrap().header.number {
            break;
        }
    }

    // Send the stop signal to the checker and wait for it to stop.
    stop_sender.send_replace(true);
    checker_task.await.unwrap().unwrap();
}

pub(crate) async fn checker_processes_pre_boojum_batches(
    (mapper_name, save_actions_mapper): (&'static str, SaveActionMapper),
    l1_batch_commit_data_generator: Arc<dyn L1BatchCommitDataGenerator>,
) {
    println!("Using save_actions_mapper={mapper_name}");

    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    let genesis_params = GenesisParams {
        protocol_version: PRE_BOOJUM_PROTOCOL_VERSION,
        ..GenesisParams::mock()
    };
    ensure_genesis_state(&mut storage, L2ChainId::default(), &genesis_params)
        .await
        .unwrap();
    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(ProtocolVersion::default())
        .await;

    let l1_batches: Vec<_> = (1..=5)
        .map(create_pre_boojum_l1_batch_with_metadata)
        .chain((6..=10).map(create_l1_batch_with_metadata))
        .collect();
    let mut commit_tx_hash_by_l1_batch = HashMap::with_capacity(l1_batches.len());
    let client = MockEthereum::default();

    for (i, l1_batch) in l1_batches.iter().enumerate() {
        let input_data = build_commit_tx_input_data(
            slice::from_ref(l1_batch),
            l1_batch_commit_data_generator.clone(),
        );
        let signed_tx = client.sign_prepared_tx(
            input_data.clone(),
            Options {
                nonce: Some(i.into()),
                ..Options::default()
            },
        );
        let signed_tx = signed_tx.unwrap();
        client.send_raw_tx(signed_tx.raw_tx).await.unwrap();
        client.execute_tx(signed_tx.hash, true, 1);

        commit_tx_hash_by_l1_batch.insert(l1_batch.header.number, signed_tx.hash);
    }

    let (l1_batch_updates_sender, mut l1_batch_updates_receiver) = mpsc::unbounded_channel();
    let checker = ConsistencyChecker {
        l1_batch_updater: Box::new(l1_batch_updates_sender),
        ..create_mock_checker(client, pool.clone(), l1_batch_commit_data_generator)
    };

    let (stop_sender, stop_receiver) = watch::channel(false);
    let checker_task = tokio::spawn(checker.run(stop_receiver));

    // Add new batches to the storage.
    for save_action in save_actions_mapper(&l1_batches) {
        save_action
            .apply(&mut storage, &commit_tx_hash_by_l1_batch)
            .await;
        tokio::time::sleep(Duration::from_millis(7)).await;
    }

    // Wait until all batches are checked.
    loop {
        let checked_batch = l1_batch_updates_receiver.recv().await.unwrap();
        if checked_batch == l1_batches.last().unwrap().header.number {
            break;
        }
    }

    // Send the stop signal to the checker and wait for it to stop.
    stop_sender.send_replace(true);
    checker_task.await.unwrap().unwrap();
}

pub async fn checker_functions_after_snapshot_recovery(
    delay_batch_insertion: bool,
    l1_batch_commit_data_generator: Arc<dyn L1BatchCommitDataGenerator>,
) {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(ProtocolVersion::default())
        .await;

    let l1_batch = create_l1_batch_with_metadata(99);

    let commit_tx_input_data = build_commit_tx_input_data(
        slice::from_ref(&l1_batch),
        l1_batch_commit_data_generator.clone(),
    );
    let client = MockEthereum::default();
    let signed_tx = client.sign_prepared_tx(
        commit_tx_input_data.clone(),
        Options {
            nonce: Some(0.into()),
            ..Options::default()
        },
    );
    let signed_tx = signed_tx.unwrap();
    let commit_tx_hash = signed_tx.hash;
    client.send_raw_tx(signed_tx.raw_tx).await.unwrap();
    client.execute_tx(commit_tx_hash, true, 1);

    let save_actions = [
        SaveAction::InsertBatch(&l1_batch),
        SaveAction::SaveMetadata(&l1_batch),
        SaveAction::InsertCommitTx(l1_batch.header.number),
    ];
    let commit_tx_hash_by_l1_batch = HashMap::from([(l1_batch.header.number, commit_tx_hash)]);

    if !delay_batch_insertion {
        for &save_action in &save_actions {
            save_action
                .apply(&mut storage, &commit_tx_hash_by_l1_batch)
                .await;
        }
    }

    let (l1_batch_updates_sender, mut l1_batch_updates_receiver) = mpsc::unbounded_channel();
    let checker = ConsistencyChecker {
        l1_batch_updater: Box::new(l1_batch_updates_sender),
        ..create_mock_checker(client, pool.clone(), l1_batch_commit_data_generator)
    };
    let (stop_sender, stop_receiver) = watch::channel(false);
    let checker_task = tokio::spawn(checker.run(stop_receiver));

    if delay_batch_insertion {
        tokio::time::sleep(Duration::from_millis(10)).await;
        for &save_action in &save_actions {
            save_action
                .apply(&mut storage, &commit_tx_hash_by_l1_batch)
                .await;
        }
    }

    // Wait until the batch is checked.
    let checked_batch = l1_batch_updates_receiver.recv().await.unwrap();
    assert_eq!(checked_batch, l1_batch.header.number);

    stop_sender.send_replace(true);
    checker_task.await.unwrap().unwrap();
}

pub(crate) async fn checker_detects_incorrect_tx_data(
    kind: IncorrectDataKind,
    snapshot_recovery: bool,
    l1_batch_commit_data_generator: Arc<dyn L1BatchCommitDataGenerator>,
) {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    if snapshot_recovery {
        storage
            .protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
    } else {
        ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
            .await
            .unwrap();
    }

    let l1_batch = create_l1_batch_with_metadata(if snapshot_recovery { 99 } else { 1 });
    let client = MockEthereum::default();
    let commit_tx_hash = kind
        .apply(&client, &l1_batch, l1_batch_commit_data_generator.clone())
        .await;
    let commit_tx_hash_by_l1_batch = HashMap::from([(l1_batch.header.number, commit_tx_hash)]);

    let save_actions = [
        SaveAction::InsertBatch(&l1_batch),
        SaveAction::SaveMetadata(&l1_batch),
        SaveAction::InsertCommitTx(l1_batch.header.number),
    ];
    for save_action in save_actions {
        save_action
            .apply(&mut storage, &commit_tx_hash_by_l1_batch)
            .await;
    }
    drop(storage);

    let checker = create_mock_checker(client, pool, l1_batch_commit_data_generator);
    let (_stop_sender, stop_receiver) = watch::channel(false);
    // The checker must stop with an error.
    tokio::time::timeout(Duration::from_secs(30), checker.run(stop_receiver))
        .await
        .expect("Timed out waiting for checker to stop")
        .unwrap_err();
}

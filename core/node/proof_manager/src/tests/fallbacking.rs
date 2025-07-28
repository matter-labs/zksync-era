use std::time::Duration;

use zksync_config::configs::proof_data_handler::ProvingMode;
use zksync_dal::{proof_manager_dal::ProvingNetwork, ConnectionPool, Core, CoreDal};
use zksync_object_store::MockObjectStore;
use zksync_proof_data_handler::{Locking, Processor};
use zksync_types::{L1BatchNumber, L2ChainId, H256};

use crate::tests::{prepare_database, test_config};

// test basic flow of proof manager with fallbacking due to acknowledgment timeout
#[tokio::test]
async fn test_fallbacking_acknowledgment_timeout() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let config = test_config();
    let blob_store = MockObjectStore::arc();

    let mut connection = connection_pool.connection().await.unwrap();

    prepare_database(&mut connection).await;

    let processor = Processor::<Locking>::new(
        blob_store,
        connection_pool,
        config.proof_generation_timeout,
        L2ChainId::new(270).unwrap(),
        ProvingMode::ProvingNetwork,
    );

    // At this point, batch should be available for proving networks, but not for prover cluster

    let batch = processor
        .lock_batch_for_proving(config.proof_generation_timeout)
        .await
        .unwrap();

    assert_eq!(batch, None);

    let batch = processor.lock_batch_for_proving_network().await.unwrap();

    assert_eq!(batch, Some(L1BatchNumber(1)));

    processor.unlock_batch(L1BatchNumber(1)).await.unwrap();

    connection
        .proof_manager_dal()
        .insert_batch(L1BatchNumber(1), "url")
        .await
        .unwrap();

    connection
        .proof_manager_dal()
        .mark_batch_as_sent(L1BatchNumber(1), H256::zero())
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;

    // After timeout of acknowledgment, batch should be fallbacked, so it will be available for prover cluster, but not for proving network

    connection
        .proof_manager_dal()
        .fallback_batches(
            config.acknowledgment_timeout,
            config.proof_generation_timeout,
            config.picking_timeout,
        )
        .await
        .unwrap();

    let batch = processor.lock_batch_for_proving_network().await.unwrap();
    assert_eq!(batch, None);

    let batch = processor
        .lock_batch_for_proving(config.proof_generation_timeout)
        .await
        .unwrap();

    assert_eq!(batch, Some(L1BatchNumber(1)));
}

// test basic flow of proof manager with fallbacking due to proving timeout
#[tokio::test]
async fn test_fallbacking_proving_timeout() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut config = test_config();
    config.acknowledgment_timeout = Duration::from_secs(20);
    let blob_store = MockObjectStore::arc();

    let mut connection = connection_pool.connection().await.unwrap();

    prepare_database(&mut connection).await;

    let processor = Processor::<Locking>::new(
        blob_store,
        connection_pool,
        config.proof_generation_timeout,
        L2ChainId::new(270).unwrap(),
        ProvingMode::ProvingNetwork,
    );

    // At this point, batch should be available for proving networks, but not for prover cluster

    let batch = processor
        .lock_batch_for_proving(config.proof_generation_timeout)
        .await
        .unwrap();
    assert_eq!(batch, None);

    let batch = processor.lock_batch_for_proving_network().await.unwrap();

    assert_eq!(batch, Some(L1BatchNumber(1)));

    processor.unlock_batch(L1BatchNumber(1)).await.unwrap();

    connection
        .proof_manager_dal()
        .insert_batch(L1BatchNumber(1), "url")
        .await
        .unwrap();

    connection
        .proof_manager_dal()
        .acknowledge_batch(L1BatchNumber(1), ProvingNetwork::Fermah)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;

    // After timeout of proving, batch should be fallbacked, so it will be available for prover cluster, but not for proving network

    connection
        .proof_manager_dal()
        .fallback_batches(
            config.acknowledgment_timeout,
            config.proof_generation_timeout,
            config.picking_timeout,
        )
        .await
        .unwrap();

    let batch = processor.lock_batch_for_proving_network().await.unwrap();
    assert_eq!(batch, None);

    let batch = processor
        .lock_batch_for_proving(config.proof_generation_timeout)
        .await
        .unwrap();

    assert_eq!(batch, Some(L1BatchNumber(1)));
}

// test basic flow of proof manager with fallbacking due to picking timeout
#[tokio::test]
async fn test_fallbacking_picking_timeout() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let config = test_config();
    let blob_store = MockObjectStore::arc();

    let mut connection = connection_pool.connection().await.unwrap();

    prepare_database(&mut connection).await;

    let processor = Processor::<Locking>::new(
        blob_store,
        connection_pool,
        config.proof_generation_timeout,
        L2ChainId::new(270).unwrap(),
        ProvingMode::ProvingNetwork,
    );

    // At this point, batch should be available for proving networks, but not for prover cluster

    let batch = processor
        .lock_batch_for_proving(config.proof_generation_timeout)
        .await
        .unwrap();

    assert_eq!(batch, None);

    let batch = processor.lock_batch_for_proving_network().await.unwrap();
    assert_eq!(batch, Some(L1BatchNumber(1)));

    processor.unlock_batch(L1BatchNumber(1)).await.unwrap();

    // After timeout of picking, batch should be fallbacked, so it will be available for prover cluster, but not for proving network

    tokio::time::sleep(Duration::from_secs(2)).await;

    connection
        .proof_manager_dal()
        .fallback_batches(
            config.acknowledgment_timeout,
            config.proof_generation_timeout,
            config.picking_timeout,
        )
        .await
        .unwrap();

    let batch = processor.lock_batch_for_proving_network().await.unwrap();
    assert_eq!(batch, None);

    let batch = processor
        .lock_batch_for_proving(config.proof_generation_timeout)
        .await
        .unwrap();

    assert_eq!(batch, Some(L1BatchNumber(1)));
}

#[tokio::test]
async fn test_fallbacking_invalid_proof() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let config = test_config();
    let blob_store = MockObjectStore::arc();

    let mut connection = connection_pool.connection().await.unwrap();

    prepare_database(&mut connection).await;

    let processor = Processor::<Locking>::new(
        blob_store,
        connection_pool,
        config.proof_generation_timeout,
        L2ChainId::new(270).unwrap(),
        ProvingMode::ProvingNetwork,
    );

    // At this point, batch should be available for proving networks, but not for prover cluster

    let batch = processor
        .lock_batch_for_proving(config.proof_generation_timeout)
        .await
        .unwrap();

    assert_eq!(batch, None);

    let batch = processor.lock_batch_for_proving_network().await.unwrap();
    assert_eq!(batch, Some(L1BatchNumber(1)));

    processor.unlock_batch(L1BatchNumber(1)).await.unwrap();

    connection
        .proof_manager_dal()
        .insert_batch(L1BatchNumber(1), "url")
        .await
        .unwrap();

    connection
        .proof_manager_dal()
        .mark_batch_as_proven(L1BatchNumber(1), false)
        .await
        .unwrap();

    // Batch should be fallbacked if the proof provided was invalid

    connection
        .proof_manager_dal()
        .fallback_batches(
            config.acknowledgment_timeout,
            config.proof_generation_timeout,
            config.picking_timeout,
        )
        .await
        .unwrap();

    let batch = processor.lock_batch_for_proving_network().await.unwrap();
    assert_eq!(batch, None);

    let batch = processor
        .lock_batch_for_proving(config.proof_generation_timeout)
        .await
        .unwrap();

    assert_eq!(batch, Some(L1BatchNumber(1)));
}

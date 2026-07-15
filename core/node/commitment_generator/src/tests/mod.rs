//! Tests for `CommitmentGenerator`.

use std::thread;

use rand::{thread_rng, Rng};
use serde::Deserialize;
use zksync_dal::Connection;
use zksync_multivm::interface::VmEvent;
use zksync_node_genesis::{insert_genesis_batch, GenesisParamsInitials};
use zksync_node_test_utils::{create_l1_batch, create_l2_block};
use zksync_types::{
    block::L1BatchTreeData, zk_evm_types::LogQuery, AccountTreeId, Address, StorageLog,
};

use super::*;

async fn seal_l1_batch(storage: &mut Connection<'_, Core>, number: L1BatchNumber) {
    let l2_block = create_l2_block(number.0);
    storage
        .blocks_dal()
        .insert_l2_block(&l2_block)
        .await
        .unwrap();
    let storage_key = StorageKey::new(
        AccountTreeId::new(Address::repeat_byte(1)),
        H256::from_low_u64_be(number.0.into()),
    );
    let storage_log = StorageLog::new_write_log(storage_key, H256::repeat_byte(0xff));
    storage
        .storage_logs_dal()
        .insert_storage_logs(l2_block.number, &[storage_log])
        .await
        .unwrap();
    storage
        .storage_logs_dedup_dal()
        .insert_initial_writes(number, &[storage_key.hashed_key()])
        .await
        .unwrap();

    let header = create_l1_batch(number.0);
    storage
        .blocks_dal()
        .insert_mock_l1_batch(&header)
        .await
        .unwrap();
    storage
        .blocks_dal()
        .mark_l2_blocks_as_executed_in_l1_batch(number)
        .await
        .unwrap();
}

async fn save_l1_batch_tree_data(storage: &mut Connection<'_, Core>, number: L1BatchNumber) {
    let tree_data = L1BatchTreeData {
        hash: H256::from_low_u64_be(number.0.into()),
        rollup_last_leaf_index: 20 + 10 * u64::from(number.0),
    };
    storage
        .blocks_dal()
        .save_l1_batch_tree_data(number, &tree_data)
        .await
        .unwrap();
}

#[derive(Debug)]
struct MockCommitmentComputer {
    delay: Duration,
}

impl MockCommitmentComputer {
    const EVENTS_QUEUE_COMMITMENT: H256 = H256::repeat_byte(1);
    const BOOTLOADER_COMMITMENT: H256 = H256::repeat_byte(2);
}

impl CommitmentComputer for MockCommitmentComputer {
    fn events_queue_commitment(
        &self,
        _events_queue: &[LogQuery],
        protocol_version: ProtocolVersionId,
    ) -> anyhow::Result<H256> {
        assert_eq!(protocol_version, ProtocolVersionId::latest());
        thread::sleep(self.delay);
        Ok(Self::EVENTS_QUEUE_COMMITMENT)
    }

    fn bootloader_initial_content_commitment(
        &self,
        _initial_bootloader_contents: &[(usize, U256)],
        protocol_version: ProtocolVersionId,
    ) -> anyhow::Result<H256> {
        assert_eq!(protocol_version, ProtocolVersionId::latest());
        thread::sleep(self.delay);
        Ok(Self::BOOTLOADER_COMMITMENT)
    }
}

fn create_commitment_generator(pool: ConnectionPool<Core>) -> CommitmentGenerator {
    let mut generator = CommitmentGenerator::new(pool, false);
    generator.computer = Arc::new(MockCommitmentComputer {
        delay: Duration::from_millis(20),
    });
    generator
}

fn processed_batch(health: &Health, expected_number: L1BatchNumber) -> bool {
    if !matches!(health.status(), HealthStatus::Ready) {
        return false;
    }
    let Some(details) = health.details() else {
        return false;
    };
    *details == serde_json::json!({ "l1_batch_number": expected_number })
}

#[tokio::test]
async fn determining_batch_range() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParamsInitials::mock())
        .await
        .unwrap();

    let mut generator = create_commitment_generator(pool.clone());
    generator.parallelism = NonZeroU32::new(4).unwrap(); // to be deterministic
    assert_eq!(generator.next_batch_range().await.unwrap(), None);

    seal_l1_batch(&mut storage, L1BatchNumber(1)).await;
    assert_eq!(generator.next_batch_range().await.unwrap(), None); // No tree data for L1 batch #1

    save_l1_batch_tree_data(&mut storage, L1BatchNumber(1)).await;
    assert_eq!(
        generator.next_batch_range().await.unwrap(),
        Some(L1BatchNumber(1)..=L1BatchNumber(1))
    );

    seal_l1_batch(&mut storage, L1BatchNumber(2)).await;
    assert_eq!(
        generator.next_batch_range().await.unwrap(),
        Some(L1BatchNumber(1)..=L1BatchNumber(1))
    );

    save_l1_batch_tree_data(&mut storage, L1BatchNumber(2)).await;
    assert_eq!(
        generator.next_batch_range().await.unwrap(),
        Some(L1BatchNumber(1)..=L1BatchNumber(2))
    );

    for number in 3..=5 {
        seal_l1_batch(&mut storage, L1BatchNumber(number)).await;
    }
    assert_eq!(
        generator.next_batch_range().await.unwrap(),
        Some(L1BatchNumber(1)..=L1BatchNumber(2))
    );

    for number in 3..=5 {
        save_l1_batch_tree_data(&mut storage, L1BatchNumber(number)).await;
    }
    // L1 batch #5 is excluded because of the parallelism limit
    assert_eq!(
        generator.next_batch_range().await.unwrap(),
        Some(L1BatchNumber(1)..=L1BatchNumber(4))
    );
}

#[tokio::test]
async fn commitment_generator_normal_operation() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParamsInitials::mock())
        .await
        .unwrap();

    let generator = create_commitment_generator(pool.clone());
    let mut health_check = generator.health_check();
    let (stop_sender, stop_receiver) = watch::channel(false);
    let generator_handle = tokio::spawn(generator.run(stop_receiver));

    for number in 1..=5 {
        let number = L1BatchNumber(number);
        seal_l1_batch(&mut storage, number).await;
        save_l1_batch_tree_data(&mut storage, number).await;
        // Wait until the batch is processed by the generator
        health_check
            .wait_for(|health| processed_batch(health, number))
            .await;
        // Check data in Postgres
        let metadata = storage
            .blocks_dal()
            .get_l1_batch_metadata(number)
            .await
            .unwrap()
            .expect("no batch metadata");
        assert_eq!(
            metadata.metadata.events_queue_commitment,
            Some(MockCommitmentComputer::EVENTS_QUEUE_COMMITMENT)
        );
        assert_eq!(
            metadata.metadata.bootloader_initial_content_commitment,
            Some(MockCommitmentComputer::BOOTLOADER_COMMITMENT)
        );
    }

    stop_sender.send_replace(true);
    generator_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn commitment_generator_bulk_processing() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParamsInitials::mock())
        .await
        .unwrap();

    for number in 1..=5 {
        seal_l1_batch(&mut storage, L1BatchNumber(number)).await;
        save_l1_batch_tree_data(&mut storage, L1BatchNumber(number)).await;
    }

    let mut generator = create_commitment_generator(pool.clone());
    generator.parallelism = NonZeroU32::new(10).unwrap(); // enough to process all batches at once
    let mut health_check = generator.health_check();
    let (stop_sender, stop_receiver) = watch::channel(false);
    let generator_handle = tokio::spawn(generator.run(stop_receiver));

    health_check
        .wait_for(|health| processed_batch(health, L1BatchNumber(5)))
        .await;
    for number in 1..=5 {
        let metadata = storage
            .blocks_dal()
            .get_l1_batch_metadata(L1BatchNumber(number))
            .await
            .unwrap()
            .expect("no batch metadata");
        assert_eq!(
            metadata.metadata.events_queue_commitment,
            Some(MockCommitmentComputer::EVENTS_QUEUE_COMMITMENT)
        );
        assert_eq!(
            metadata.metadata.bootloader_initial_content_commitment,
            Some(MockCommitmentComputer::BOOTLOADER_COMMITMENT)
        );
    }

    stop_sender.send_replace(true);
    generator_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn commitment_generator_with_tree_emulation() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParamsInitials::mock())
        .await
        .unwrap();
    drop(storage);

    // Emulates adding new batches to the storage.
    let new_batches_pool = pool.clone();
    let new_batches_handle = tokio::spawn(async move {
        for number in 1..=10 {
            let sleep_delay = Duration::from_millis(thread_rng().gen_range(1..20));
            tokio::time::sleep(sleep_delay).await;
            let mut storage = new_batches_pool.connection().await.unwrap();
            seal_l1_batch(&mut storage, L1BatchNumber(number)).await;
        }
    });

    let tree_emulator_pool = pool.clone();
    let tree_emulator_handle = tokio::spawn(async move {
        for number in 1..=10 {
            let mut storage = tree_emulator_pool.connection().await.unwrap();
            while storage
                .blocks_dal()
                .get_sealed_l1_batch_number()
                .await
                .unwrap()
                < Some(L1BatchNumber(number))
            {
                let sleep_delay = Duration::from_millis(thread_rng().gen_range(5..10));
                tokio::time::sleep(sleep_delay).await;
            }
            save_l1_batch_tree_data(&mut storage, L1BatchNumber(number)).await;
        }
    });

    let mut generator = create_commitment_generator(pool.clone());
    generator.parallelism = NonZeroU32::new(10).unwrap(); // enough to process all batches at once
    let mut health_check = generator.health_check();
    let (stop_sender, stop_receiver) = watch::channel(false);
    let generator_handle = tokio::spawn(generator.run(stop_receiver));

    health_check
        .wait_for(|health| processed_batch(health, L1BatchNumber(10)))
        .await;

    new_batches_handle.await.unwrap();
    tree_emulator_handle.await.unwrap();
    stop_sender.send_replace(true);
    generator_handle.await.unwrap().unwrap();
}

#[derive(Debug, Deserialize)]
struct SerdeVmEvent {
    location: (L1BatchNumber, u32),
    address: Address,
    indexed_topics: Vec<H256>,
    value: Vec<u8>,
}

impl From<SerdeVmEvent> for VmEvent {
    fn from(event: SerdeVmEvent) -> VmEvent {
        VmEvent {
            location: event.location,
            address: event.address,
            indexed_topics: event.indexed_topics,
            value: event.value,
        }
    }
}

#[test]
fn test_convert_vm_events_to_log_queries() {
    let cases: Vec<serde_json::Value> = vec![
        serde_json::from_str(include_str!(
            "./test_vectors/event_with_1_topic_and_long_value.json"
        ))
        .unwrap(),
        serde_json::from_str(include_str!("./test_vectors/event_with_2_topics.json")).unwrap(),
        serde_json::from_str(include_str!("./test_vectors/event_with_3_topics.json")).unwrap(),
        serde_json::from_str(include_str!("./test_vectors/event_with_4_topics.json")).unwrap(),
        serde_json::from_str(include_str!("./test_vectors/event_with_value_len_1.json")).unwrap(),
    ];

    for case in cases {
        let event: SerdeVmEvent = serde_json::from_value(case["event"].clone()).unwrap();
        let expected_list: Vec<LogQuery> = serde_json::from_value(case["list"].clone()).unwrap();

        let actual_list = convert_vm_events_to_log_queries(&[event.into()]);
        assert_eq!(actual_list, expected_list);
    }
}

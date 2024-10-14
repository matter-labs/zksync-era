use rand::Rng as _;
use zksync_consensus_roles::{attester, validator};
use zksync_consensus_storage::ReplicaState;
use zksync_types::{
    block::L1BatchTreeData,
    commitment::{L1BatchCommitmentArtifacts, L1BatchCommitmentHash},
    ProtocolVersion,
};

use super::*;
use crate::{
    tests::{create_l1_batch_header, create_l2_block_header},
    ConnectionPool, Core, CoreDal,
};

#[tokio::test]
async fn replica_state_read_write() {
    let rng = &mut rand::thread_rng();
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    assert_eq!(None, conn.consensus_dal().global_config().await.unwrap());
    for n in 0..3 {
        let setup = validator::testonly::Setup::new(rng, 3);
        let mut genesis = (*setup.genesis).clone();
        genesis.fork_number = validator::ForkNumber(n);
        let cfg = GlobalConfig {
            genesis: genesis.with_hash(),
            registry_address: Some(rng.gen()),
            seed_peers: [].into(), // TODO: rng.gen() for Host
        };
        conn.consensus_dal()
            .try_update_global_config(&cfg)
            .await
            .unwrap();
        assert_eq!(
            cfg,
            conn.consensus_dal().global_config().await.unwrap().unwrap()
        );
        assert_eq!(
            ReplicaState::default(),
            conn.consensus_dal().replica_state().await.unwrap()
        );
        for _ in 0..5 {
            let want: ReplicaState = rng.gen();
            conn.consensus_dal().set_replica_state(&want).await.unwrap();
            assert_eq!(
                cfg,
                conn.consensus_dal().global_config().await.unwrap().unwrap()
            );
            assert_eq!(want, conn.consensus_dal().replica_state().await.unwrap());
        }
    }
}

#[tokio::test]
async fn test_batch_certificate() {
    let rng = &mut rand::thread_rng();
    let setup = validator::testonly::Setup::new(rng, 3);
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    let cfg = GlobalConfig {
        genesis: setup.genesis.clone(),
        registry_address: Some(rng.gen()),
        seed_peers: [].into(),
    };
    conn.consensus_dal()
        .try_update_global_config(&cfg)
        .await
        .unwrap();

    let make_cert = |number: attester::BatchNumber, hash: attester::BatchHash| {
        let m = attester::Batch {
            genesis: setup.genesis.hash(),
            hash,
            number,
        };
        let mut sigs = attester::MultiSig::default();
        for k in &setup.attester_keys {
            sigs.add(k.public(), k.sign_msg(m.clone()).sig);
        }
        attester::BatchQC {
            message: m,
            signatures: sigs,
        }
    };

    // Required for inserting l2 blocks
    conn.protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();

    // Insert some mock L2 blocks and L1 batches
    let mut block_number = 0;
    let mut batch_number = 0;
    for _ in 0..3 {
        for _ in 0..3 {
            block_number += 1;
            let l2_block = create_l2_block_header(block_number);
            conn.blocks_dal().insert_l2_block(&l2_block).await.unwrap();
        }
        batch_number += 1;
        let l1_batch = create_l1_batch_header(batch_number);
        conn.blocks_dal()
            .insert_mock_l1_batch(&l1_batch)
            .await
            .unwrap();
        conn.blocks_dal()
            .save_l1_batch_tree_data(
                l1_batch.number,
                &L1BatchTreeData {
                    hash: rng.gen(),
                    rollup_last_leaf_index: rng.gen(),
                },
            )
            .await
            .unwrap();
        conn.blocks_dal()
            .save_l1_batch_commitment_artifacts(
                l1_batch.number,
                &L1BatchCommitmentArtifacts {
                    commitment_hash: L1BatchCommitmentHash {
                        pass_through_data: rng.gen(),
                        aux_output: rng.gen(),
                        meta_parameters: rng.gen(),
                        commitment: rng.gen(),
                    },
                    l2_l1_merkle_root: rng.gen(),
                    compressed_state_diffs: None,
                    compressed_initial_writes: None,
                    compressed_repeated_writes: None,
                    zkporter_is_available: false,
                    aux_commitments: None,
                    aggregation_root: rng.gen(),
                    local_root: rng.gen(),
                    state_diff_hash: rng.gen(),
                },
            )
            .await
            .unwrap();
        conn.blocks_dal()
            .mark_l2_blocks_as_executed_in_l1_batch(l1_batch.number)
            .await
            .unwrap();
    }

    let n = attester::BatchNumber(batch_number.into());

    // Insert a batch certificate for the last L1 batch.
    let hash = batch_hash(&conn.consensus_dal().batch_info(n).await.unwrap().unwrap());
    let want = make_cert(n, hash);
    conn.consensus_dal()
        .upsert_attester_committee(n, setup.genesis.attesters.as_ref().unwrap())
        .await
        .unwrap();
    conn.consensus_dal()
        .insert_batch_certificate(&want)
        .await
        .unwrap();

    // Reinserting a cert should fail.
    assert!(conn
        .consensus_dal()
        .insert_batch_certificate(&make_cert(n, hash))
        .await
        .is_err());

    // Retrieve the latest certificate.
    let got_n = conn
        .consensus_dal()
        .last_batch_certificate_number()
        .await
        .unwrap()
        .unwrap();
    let got = conn
        .consensus_dal()
        .batch_certificate(got_n)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got, want);

    // Try insert batch certificate for non-existing batch
    assert!(conn
        .consensus_dal()
        .insert_batch_certificate(&make_cert(n.next(), rng.gen()))
        .await
        .is_err());
}

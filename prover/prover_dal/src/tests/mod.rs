use crate::prover_dal::{GetProverJobsParams, ProverDal};
use crate::witness_generator_dal::WitnessGeneratorDal;

fn create_circuits() -> Vec<(&'static str, String)> {
    vec![
        ("Main VM", "1_0_Main VM_BasicCircuits.bin".to_owned()),
        ("SHA256", "1_1_SHA256_BasicCircuits.bin".to_owned()),
        (
            "Code decommitter",
            "1_2_Code decommitter_BasicCircuits.bin".to_owned(),
        ),
        (
            "Log demuxer",
            "1_3_Log demuxer_BasicCircuits.bin".to_owned(),
        ),
    ]
}

#[tokio::test]
async fn test_duplicate_insert_prover_jobs() {
    let connection_pool = MainConnectionPool::test_pool().await;
    let storage = &mut connection_pool.access_storage().await.unwrap();
    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(Default::default())
        .await;
    storage
        .protocol_versions_dal()
        .save_prover_protocol_version(Default::default())
        .await;
    let block_number = 1;
    let header = L1BatchHeader::new(
        L1BatchNumber(block_number),
        0,
        Default::default(),
        Default::default(),
        Default::default(),
    );
    storage
        .blocks_dal()
        .insert_l1_batch(&header, &[], Default::default(), &[], &[])
        .await
        .unwrap();

    let mut prover_dal = ProverDal { storage };
    let circuits = create_circuits();
    let l1_batch_number = L1BatchNumber(block_number);
    prover_dal
        .insert_prover_jobs(
            l1_batch_number,
            circuits.clone(),
            AggregationRound::BasicCircuits,
            ProtocolVersionId::latest() as i32,
        )
        .await;

    // try inserting the same jobs again to ensure it does not panic
    prover_dal
        .insert_prover_jobs(
            l1_batch_number,
            circuits.clone(),
            AggregationRound::BasicCircuits,
            ProtocolVersionId::latest() as i32,
        )
        .await;

    let prover_jobs_params = GetProverJobsParams {
        statuses: None,
        blocks: Some(std::ops::Range {
            start: l1_batch_number,
            end: l1_batch_number + 1,
        }),
        limit: None,
        desc: false,
        round: None,
    };
    let jobs = prover_dal.get_jobs(prover_jobs_params).await.unwrap();
    assert_eq!(circuits.len(), jobs.len());
}

#[tokio::test]
async fn test_requeue_prover_jobs() {
    let connection_pool = MainConnectionPool::test_pool().await;
    let storage = &mut connection_pool.access_storage().await.unwrap();
    let protocol_version = ProtocolVersion::default();
    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(protocol_version)
        .await;
    storage
        .protocol_versions_dal()
        .save_prover_protocol_version(Default::default())
        .await;
    let block_number = 1;
    let header = L1BatchHeader::new(
        L1BatchNumber(block_number),
        0,
        Default::default(),
        Default::default(),
        ProtocolVersionId::latest(),
    );
    storage
        .blocks_dal()
        .insert_l1_batch(&header, &[], Default::default(), &[], &[])
        .await
        .unwrap();

    let mut prover_dal = ProverDal { storage };
    let circuits = create_circuits();
    let l1_batch_number = L1BatchNumber(block_number);
    prover_dal
        .insert_prover_jobs(
            l1_batch_number,
            circuits,
            AggregationRound::BasicCircuits,
            ProtocolVersionId::latest() as i32,
        )
        .await;

    // take all jobs from prover_job table
    for _ in 1..=4 {
        let job = prover_dal
            .get_next_prover_job(&[ProtocolVersionId::latest()])
            .await;
        assert!(job.is_some());
    }
    let job = prover_dal
        .get_next_prover_job(&[ProtocolVersionId::latest()])
        .await;
    assert!(job.is_none());
    // re-queue jobs
    let stuck_jobs = prover_dal
        .requeue_stuck_jobs(Duration::from_secs(0), 10)
        .await;
    assert_eq!(4, stuck_jobs.len());
    // re-check that all jobs can be taken again
    for _ in 1..=4 {
        let job = prover_dal
            .get_next_prover_job(&[ProtocolVersionId::latest()])
            .await;
        assert!(job.is_some());
    }
}

#[tokio::test]
async fn test_move_leaf_aggregation_jobs_from_waiting_to_queued() {
    let connection_pool = MainConnectionPool::test_pool().await;
    let storage = &mut connection_pool.access_storage().await.unwrap();
    let protocol_version = ProtocolVersion::default();
    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(protocol_version)
        .await;
    storage
        .protocol_versions_dal()
        .save_prover_protocol_version(Default::default())
        .await;
    let block_number = 1;
    let header = L1BatchHeader::new(
        L1BatchNumber(block_number),
        0,
        Default::default(),
        Default::default(),
        ProtocolVersionId::latest(),
    );
    storage
        .blocks_dal()
        .insert_l1_batch(&header, &[], Default::default(), &[], &[])
        .await
        .unwrap();

    let mut prover_dal = ProverDal { storage };
    let circuits = create_circuits();
    let l1_batch_number = L1BatchNumber(block_number);
    prover_dal
        .insert_prover_jobs(
            l1_batch_number,
            circuits.clone(),
            AggregationRound::BasicCircuits,
            ProtocolVersionId::latest() as i32,
        )
        .await;
    let prover_jobs_params = get_default_prover_jobs_params(l1_batch_number);
    let jobs = prover_dal.get_jobs(prover_jobs_params).await;
    let job_ids: Vec<u32> = jobs.unwrap().into_iter().map(|job| job.id).collect();

    let proof = get_sample_proof();

    // mark all basic circuit proofs as successful.
    for id in job_ids.iter() {
        prover_dal
            .save_proof(*id, Duration::from_secs(0), proof.clone(), "unit-test")
            .await
            .unwrap();
    }
    let mut witness_generator_dal = WitnessGeneratorDal { storage };

    witness_generator_dal
        .create_aggregation_jobs(
            l1_batch_number,
            "basic_circuits_1.bin",
            "basic_circuits_inputs_1.bin",
            circuits.len(),
            "scheduler_witness_1.bin",
            ProtocolVersionId::latest() as i32,
        )
        .await;

    // move the leaf aggregation job to be queued
    witness_generator_dal
        .move_leaf_aggregation_jobs_from_waiting_to_queued()
        .await;

    // Ensure get-next job gives the leaf aggregation witness job
    let job = witness_generator_dal
        .get_next_leaf_aggregation_witness_job(
            Duration::from_secs(0),
            10,
            u32::MAX,
            &[ProtocolVersionId::latest()],
        )
        .await;
    assert_eq!(l1_batch_number, job.unwrap().block_number);
}

#[tokio::test]
async fn test_move_node_aggregation_jobs_from_waiting_to_queued() {
    let connection_pool = MainConnectionPool::test_pool().await;
    let storage = &mut connection_pool.access_storage().await.unwrap();
    let protocol_version = ProtocolVersion::default();
    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(protocol_version)
        .await;
    storage
        .protocol_versions_dal()
        .save_prover_protocol_version(Default::default())
        .await;
    let block_number = 1;
    let header = L1BatchHeader::new(
        L1BatchNumber(block_number),
        0,
        Default::default(),
        Default::default(),
        ProtocolVersionId::latest(),
    );
    storage
        .blocks_dal()
        .insert_l1_batch(&header, &[], Default::default(), &[], &[])
        .await
        .unwrap();

    let mut prover_dal = ProverDal { storage };
    let circuits = create_circuits();
    let l1_batch_number = L1BatchNumber(block_number);
    prover_dal
        .insert_prover_jobs(
            l1_batch_number,
            circuits.clone(),
            AggregationRound::LeafAggregation,
            ProtocolVersionId::latest() as i32,
        )
        .await;
    let prover_jobs_params = get_default_prover_jobs_params(l1_batch_number);
    let jobs = prover_dal.get_jobs(prover_jobs_params).await;
    let job_ids: Vec<u32> = jobs.unwrap().into_iter().map(|job| job.id).collect();

    let proof = get_sample_proof();
    // mark all leaf aggregation circuit proofs as successful.
    for id in job_ids {
        prover_dal
            .save_proof(id, Duration::from_secs(0), proof.clone(), "unit-test")
            .await
            .unwrap();
    }
    let mut witness_generator_dal = WitnessGeneratorDal { storage };

    witness_generator_dal
        .create_aggregation_jobs(
            l1_batch_number,
            "basic_circuits_1.bin",
            "basic_circuits_inputs_1.bin",
            circuits.len(),
            "scheduler_witness_1.bin",
            ProtocolVersionId::latest() as i32,
        )
        .await;
    witness_generator_dal
        .save_leaf_aggregation_artifacts(
            l1_batch_number,
            circuits.len(),
            "leaf_layer_subqueues_1.bin",
            "aggregation_outputs_1.bin",
        )
        .await;

    // move the leaf aggregation job to be queued
    witness_generator_dal
        .move_node_aggregation_jobs_from_waiting_to_queued()
        .await;

    // Ensure get-next job gives the node aggregation witness job
    let job = witness_generator_dal
        .get_next_node_aggregation_witness_job(
            Duration::from_secs(0),
            10,
            u32::MAX,
            &[ProtocolVersionId::latest()],
        )
        .await;
    assert_eq!(l1_batch_number, job.unwrap().block_number);
}

#[tokio::test]
async fn test_move_scheduler_jobs_from_waiting_to_queued() {
    let connection_pool = MainConnectionPool::test_pool().await;
    let storage = &mut connection_pool.access_storage().await.unwrap();
    let protocol_version = ProtocolVersion::default();
    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(protocol_version)
        .await;
    storage
        .protocol_versions_dal()
        .save_prover_protocol_version(Default::default())
        .await;
    let block_number = 1;
    let header = L1BatchHeader::new(
        L1BatchNumber(block_number),
        0,
        Default::default(),
        Default::default(),
        ProtocolVersionId::latest(),
    );
    storage
        .blocks_dal()
        .insert_l1_batch(&header, &[], Default::default(), &[], &[])
        .await
        .unwrap();

    let mut prover_dal = ProverDal { storage };
    let circuits = vec![(
        "Node aggregation",
        "1_0_Node aggregation_NodeAggregation.bin".to_owned(),
    )];
    let l1_batch_number = L1BatchNumber(block_number);
    prover_dal
        .insert_prover_jobs(
            l1_batch_number,
            circuits.clone(),
            AggregationRound::NodeAggregation,
            ProtocolVersionId::latest() as i32,
        )
        .await;
    let prover_jobs_params = get_default_prover_jobs_params(l1_batch_number);
    let jobs = prover_dal.get_jobs(prover_jobs_params).await;
    let job_ids: Vec<u32> = jobs.unwrap().into_iter().map(|job| job.id).collect();

    let proof = get_sample_proof();
    // mark node aggregation circuit proofs as successful.
    for id in &job_ids {
        prover_dal
            .save_proof(*id, Duration::from_secs(0), proof.clone(), "unit-test")
            .await
            .unwrap();
    }
    let mut witness_generator_dal = WitnessGeneratorDal { storage };

    witness_generator_dal
        .create_aggregation_jobs(
            l1_batch_number,
            "basic_circuits_1.bin",
            "basic_circuits_inputs_1.bin",
            circuits.len(),
            "scheduler_witness_1.bin",
            ProtocolVersionId::latest() as i32,
        )
        .await;
    witness_generator_dal
        .save_node_aggregation_artifacts(l1_batch_number, "final_node_aggregations_1.bin")
        .await;

    // move the leaf aggregation job to be queued
    witness_generator_dal
        .move_scheduler_jobs_from_waiting_to_queued()
        .await;

    // Ensure get-next job gives the scheduler witness job
    let job = witness_generator_dal
        .get_next_scheduler_witness_job(
            Duration::from_secs(0),
            10,
            u32::MAX,
            &[ProtocolVersionId::latest()],
        )
        .await;
    assert_eq!(l1_batch_number, job.unwrap().block_number);
}

fn get_default_prover_jobs_params(l1_batch_number: L1BatchNumber) -> GetProverJobsParams {
    GetProverJobsParams {
        statuses: None,
        blocks: Some(std::ops::Range {
            start: l1_batch_number,
            end: l1_batch_number + 1,
        }),
        limit: None,
        desc: false,
        round: None,
    }
}

fn get_sample_proof() -> Vec<u8> {
    let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| ".".into());
    fs::read(format!("{}/etc/prover-test-data/proof.bin", zksync_home))
        .expect("Failed reading test proof file")
}


use zksync_config::GenesisConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};

use super::*;

#[tokio::test]
async fn running_genesis() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    conn.blocks_dal().delete_genesis().await.unwrap();

    let params = GenesisParamsInitials::mock();

    insert_genesis_batch(&mut conn, &params).await.unwrap();

    assert!(!conn.blocks_dal().is_genesis_needed().await.unwrap());
    let metadata = conn
        .blocks_dal()
        .get_l1_batch_metadata(L1BatchNumber(0))
        .await
        .unwrap();
    let root_hash = metadata.unwrap().metadata.root_hash;
    assert_ne!(root_hash, H256::zero());

    // Check that `genesis is not needed`
    assert!(!conn.blocks_dal().is_genesis_needed().await.unwrap());
}

#[tokio::test]
async fn running_genesis_with_big_chain_id() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    conn.blocks_dal().delete_genesis().await.unwrap();

    let params = GenesisParams::load_genesis_params(GenesisConfig {
        l2_chain_id: L2ChainId::max(),
        ..mock_genesis_config()
    })
    .unwrap()
    .into();
    insert_genesis_batch(&mut conn, &params).await.unwrap();

    assert!(!conn.blocks_dal().is_genesis_needed().await.unwrap());
    let metadata = conn
        .blocks_dal()
        .get_l1_batch_metadata(L1BatchNumber(0))
        .await;
    let root_hash = metadata.unwrap().unwrap().metadata.root_hash;
    assert_ne!(root_hash, H256::zero());
}

#[tokio::test]
async fn running_genesis_with_non_latest_protocol_version() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    let params = GenesisParams::load_genesis_params(GenesisConfig {
        protocol_version: Some(ProtocolSemanticVersion {
            minor: ProtocolVersionId::Version10,
            patch: 0.into(),
        }),
        ..mock_genesis_config()
    })
    .unwrap()
    .into();

    insert_genesis_batch(&mut conn, &params).await.unwrap();
    assert!(!conn.blocks_dal().is_genesis_needed().await.unwrap());
}

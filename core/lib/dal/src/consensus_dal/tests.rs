use rand::Rng as _;
use zksync_consensus_roles::validator;
use zksync_consensus_storage::ReplicaState;

use super::*;
use crate::{ConnectionPool, Core, CoreDal};

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

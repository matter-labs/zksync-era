use rand::Rng;
use zksync_config::testonly;
use zksync_protobuf_config::testonly::{encode_decode, FmtConv};

use super::*;

impl testonly::RandomConfig for TempConfigStore {
    fn sample(g: &mut testonly::Gen<impl Rng>) -> Self {
        Self {
            postgres_config: g.gen(),
            health_check_config: g.gen(),
            merkle_tree_api_config: g.gen(),
            web3_json_rpc_config: g.gen(),
            circuit_breaker_config: g.gen(),
            mempool_config: g.gen(),
            network_config: g.gen(),
            operations_manager_config: g.gen(),
            state_keeper_config: g.gen(),
            house_keeper_config: g.gen(),
            fri_proof_compressor_config: g.gen(),
            fri_prover_config: g.gen(),
            fri_prover_group_config: g.gen(),
            fri_witness_generator_config: g.gen(),
            prometheus_config: g.gen(),
            proof_data_handler_config: g.gen(),
            witness_generator_config: g.gen(),
            api_config: g.gen(),
            contracts_config: g.gen(),
            db_config: g.gen(),
            eth_client_config: g.gen(),
            eth_sender_config: g.gen(),
            eth_watch_config: g.gen(),
            gas_adjuster_config: g.gen(),
            object_store_config: g.gen(),
            consensus_config: g.gen(),
        }
    }
}

#[test]
fn test_encoding() {
    let rng = &mut rand::thread_rng();
    encode_decode::<FmtConv<TempConfigStore>>(rng);
}

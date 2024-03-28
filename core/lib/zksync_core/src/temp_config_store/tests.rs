use rand::{distributions::Distribution, Rng};
use zksync_consensus_utils::EncodeDist;
use zksync_protobuf::testonly::{test_encode_all_formats, FmtConv};

use super::*;

impl Distribution<TempConfigStore> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> TempConfigStore {
        TempConfigStore {
            postgres_config: self.sample(rng),
            health_check_config: self.sample(rng),
            merkle_tree_api_config: self.sample(rng),
            web3_json_rpc_config: self.sample(rng),
            circuit_breaker_config: self.sample(rng),
            mempool_config: self.sample(rng),
            network_config: self.sample(rng),
            operations_manager_config: self.sample(rng),
            state_keeper_config: self.sample(rng),
            house_keeper_config: self.sample(rng),
            fri_proof_compressor_config: self.sample(rng),
            fri_prover_config: self.sample(rng),
            fri_prover_group_config: self.sample(rng),
            fri_witness_generator_config: self.sample(rng),
            prometheus_config: self.sample(rng),
            proof_data_handler_config: self.sample(rng),
            witness_generator_config: self.sample(rng),
            api_config: self.sample(rng),
            contracts_config: self.sample(rng),
            db_config: self.sample(rng),
            eth_client_config: self.sample(rng),
            eth_sender_config: self.sample(rng),
            eth_watch_config: self.sample(rng),
            gas_adjuster_config: self.sample(rng),
            object_store_config: self.sample(rng),
            consensus_config: self.sample(rng),
        }
    }
}

#[test]
fn test_encoding() {
    let rng = &mut rand::thread_rng();
    test_encode_all_formats::<FmtConv<TempConfigStore>>(rng);
}

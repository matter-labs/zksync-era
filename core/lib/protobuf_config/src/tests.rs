use zksync_protobuf::testonly::{test_encode_all_formats, ReprConv};

use crate::proto;

/// Tests config <-> proto (boilerplate) conversions.
#[test]
fn test_encoding() {
    let rng = &mut rand::thread_rng();
    test_encode_all_formats::<ReprConv<proto::alerts::Alerts>>(rng);
    test_encode_all_formats::<ReprConv<proto::api::Web3JsonRpc>>(rng);
    test_encode_all_formats::<ReprConv<proto::api::ContractVerificationApi>>(rng);
    test_encode_all_formats::<ReprConv<proto::api::HealthCheck>>(rng);
    test_encode_all_formats::<ReprConv<proto::api::MerkleTreeApi>>(rng);
    test_encode_all_formats::<ReprConv<proto::api::Api>>(rng);
    test_encode_all_formats::<ReprConv<proto::utils::Prometheus>>(rng);
    test_encode_all_formats::<ReprConv<proto::chain::EthNetwork>>(rng);
    test_encode_all_formats::<ReprConv<proto::chain::StateKeeper>>(rng);
    test_encode_all_formats::<ReprConv<proto::chain::OperationsManager>>(rng);
    test_encode_all_formats::<ReprConv<proto::chain::Mempool>>(rng);
    test_encode_all_formats::<ReprConv<proto::chain::CircuitBreaker>>(rng);
    test_encode_all_formats::<ReprConv<proto::contract_verifier::ContractVerifier>>(rng);
    test_encode_all_formats::<ReprConv<proto::contracts::Contracts>>(rng);
    test_encode_all_formats::<ReprConv<proto::database::MerkleTree>>(rng);
    test_encode_all_formats::<ReprConv<proto::database::Db>>(rng);
    test_encode_all_formats::<ReprConv<proto::database::Postgres>>(rng);
    test_encode_all_formats::<ReprConv<proto::eth_client::EthClient>>(rng);
    test_encode_all_formats::<ReprConv<proto::eth_sender::EthSender>>(rng);
    test_encode_all_formats::<ReprConv<proto::eth_sender::Sender>>(rng);
    test_encode_all_formats::<ReprConv<proto::eth_sender::GasAdjuster>>(rng);
    test_encode_all_formats::<ReprConv<proto::eth_watch::EthWatch>>(rng);
    test_encode_all_formats::<ReprConv<proto::fri_proof_compressor::FriProofCompressor>>(rng);
    test_encode_all_formats::<ReprConv<proto::fri_prover::FriProver>>(rng);
    test_encode_all_formats::<ReprConv<proto::fri_prover_gateway::FriProverGateway>>(rng);
    test_encode_all_formats::<ReprConv<proto::fri_prover_group::FriProverGroup>>(rng);
    test_encode_all_formats::<ReprConv<proto::fri_witness_generator::FriWitnessGenerator>>(rng);
    test_encode_all_formats::<
        ReprConv<proto::fri_witness_vector_generator::FriWitnessVectorGenerator>,
    >(rng);
    test_encode_all_formats::<ReprConv<proto::house_keeper::HouseKeeper>>(rng);
    test_encode_all_formats::<ReprConv<proto::object_store::ObjectStore>>(rng);
    test_encode_all_formats::<ReprConv<proto::proof_data_handler::ProofDataHandler>>(rng);
    test_encode_all_formats::<ReprConv<proto::snapshot_creator::SnapshotsCreator>>(rng);
    test_encode_all_formats::<ReprConv<proto::witness_generator::WitnessGenerator>>(rng);
    test_encode_all_formats::<ReprConv<proto::observability::Observability>>(rng);
}

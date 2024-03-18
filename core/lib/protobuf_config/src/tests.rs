use crate::{
    proto,
    testonly::{encode_decode, ReprConv},
};

/// Tests config <-> proto (boilerplate) conversions.
#[test]
fn test_encoding() {
    let rng = &mut rand::thread_rng();
    encode_decode::<ReprConv<proto::alerts::Alerts>>(rng);
    encode_decode::<ReprConv<proto::api::Web3JsonRpc>>(rng);
    encode_decode::<ReprConv<proto::api::ContractVerificationApi>>(rng);
    encode_decode::<ReprConv<proto::api::HealthCheck>>(rng);
    encode_decode::<ReprConv<proto::api::MerkleTreeApi>>(rng);
    encode_decode::<ReprConv<proto::api::Api>>(rng);
    encode_decode::<ReprConv<proto::utils::Prometheus>>(rng);
    encode_decode::<ReprConv<proto::chain::EthNetwork>>(rng);
    encode_decode::<ReprConv<proto::chain::StateKeeper>>(rng);
    encode_decode::<ReprConv<proto::chain::OperationsManager>>(rng);
    encode_decode::<ReprConv<proto::chain::Mempool>>(rng);
    encode_decode::<ReprConv<proto::chain::CircuitBreaker>>(rng);
    encode_decode::<ReprConv<proto::contract_verifier::ContractVerifier>>(rng);
    encode_decode::<ReprConv<proto::contracts::Contracts>>(rng);
    encode_decode::<ReprConv<proto::database::MerkleTree>>(rng);
    encode_decode::<ReprConv<proto::database::Db>>(rng);
    encode_decode::<ReprConv<proto::database::Postgres>>(rng);
    encode_decode::<ReprConv<proto::eth_client::EthClient>>(rng);
    encode_decode::<ReprConv<proto::eth_sender::EthSender>>(rng);
    encode_decode::<ReprConv<proto::eth_sender::Sender>>(rng);
    encode_decode::<ReprConv<proto::eth_sender::GasAdjuster>>(rng);
    encode_decode::<ReprConv<proto::eth_watch::EthWatch>>(rng);
    encode_decode::<ReprConv<proto::fri_proof_compressor::FriProofCompressor>>(rng);
    encode_decode::<ReprConv<proto::fri_prover::FriProver>>(rng);
    encode_decode::<ReprConv<proto::fri_prover_gateway::FriProverGateway>>(rng);
    encode_decode::<ReprConv<proto::fri_prover_group::CircuitIdRoundTuple>>(rng);
    encode_decode::<ReprConv<proto::fri_prover_group::FriProverGroup>>(rng);
    encode_decode::<ReprConv<proto::fri_witness_generator::FriWitnessGenerator>>(rng);
    encode_decode::<ReprConv<proto::fri_witness_vector_generator::FriWitnessVectorGenerator>>(rng);
    encode_decode::<ReprConv<proto::house_keeper::HouseKeeper>>(rng);
    encode_decode::<ReprConv<proto::object_store::ObjectStore>>(rng);
    encode_decode::<ReprConv<proto::proof_data_handler::ProofDataHandler>>(rng);
    encode_decode::<ReprConv<proto::snapshot_creator::SnapshotsCreator>>(rng);
    encode_decode::<ReprConv<proto::witness_generator::WitnessGenerator>>(rng);
    encode_decode::<ReprConv<proto::observability::Observability>>(rng);
}

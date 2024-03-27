use crate::{
    proto,
    testonly::{encode_decode, ReprConv},
};

/// Tests config <-> proto (boilerplate) conversions.
#[test]
fn test_encoding() {
    let rng = &mut rand::thread_rng();
    encode_decode::<ReprConv<proto::api::Web3JsonRpc>>(rng);
    encode_decode::<ReprConv<proto::api::HealthCheck>>(rng);
    encode_decode::<ReprConv<proto::api::MerkleTreeApi>>(rng);
    encode_decode::<ReprConv<proto::api::Api>>(rng);
    encode_decode::<ReprConv<proto::utils::Prometheus>>(rng);
    encode_decode::<ReprConv<proto::chain::StateKeeper>>(rng);
    encode_decode::<ReprConv<proto::chain::OperationsManager>>(rng);
    encode_decode::<ReprConv<proto::chain::Mempool>>(rng);
    encode_decode::<ReprConv<proto::contract_verifier::ContractVerifier>>(rng);
    encode_decode::<ReprConv<proto::contracts::Contracts>>(rng);
    encode_decode::<ReprConv<proto::database::MerkleTree>>(rng);
    encode_decode::<ReprConv<proto::database::Db>>(rng);
    encode_decode::<ReprConv<proto::database::Postgres>>(rng);
    encode_decode::<ReprConv<proto::eth::Eth>>(rng);
    encode_decode::<ReprConv<proto::prover::ProofCompressor>>(rng);
    encode_decode::<ReprConv<proto::prover::Prover>>(rng);
    encode_decode::<ReprConv<proto::prover::ProverGateway>>(rng);
    encode_decode::<ReprConv<proto::prover::CircuitIdRoundTuple>>(rng);
    encode_decode::<ReprConv<proto::prover::ProverGroup>>(rng);
    encode_decode::<ReprConv<proto::prover::WitnessGenerator>>(rng);
    encode_decode::<ReprConv<proto::prover::WitnessVectorGenerator>>(rng);
    encode_decode::<ReprConv<proto::house_keeper::HouseKeeper>>(rng);
    encode_decode::<ReprConv<proto::object_store::ObjectStore>>(rng);
    encode_decode::<ReprConv<proto::prover::ProofDataHandler>>(rng);
    encode_decode::<ReprConv<proto::snapshot_creator::SnapshotsCreator>>(rng);
    encode_decode::<ReprConv<proto::observability::Observability>>(rng);
}

use zksync_protobuf::testonly::{test_encode_all_formats, ReprConv};

use crate::proto;

/// Tests config <-> proto (boilerplate) conversions.
#[test]
fn test_encoding() {
    let rng = &mut rand::thread_rng();
    test_encode_all_formats::<ReprConv<proto::api::Web3JsonRpc>>(rng);
    test_encode_all_formats::<ReprConv<proto::api::HealthCheck>>(rng);
    test_encode_all_formats::<ReprConv<proto::api::MerkleTreeApi>>(rng);
    test_encode_all_formats::<ReprConv<proto::api::Api>>(rng);
    test_encode_all_formats::<ReprConv<proto::utils::Prometheus>>(rng);
    test_encode_all_formats::<ReprConv<proto::chain::StateKeeper>>(rng);
    test_encode_all_formats::<ReprConv<proto::chain::OperationsManager>>(rng);
    test_encode_all_formats::<ReprConv<proto::chain::Mempool>>(rng);
    test_encode_all_formats::<ReprConv<proto::contract_verifier::ContractVerifier>>(rng);
    test_encode_all_formats::<ReprConv<proto::contracts::Contracts>>(rng);
    test_encode_all_formats::<ReprConv<proto::database::MerkleTree>>(rng);
    test_encode_all_formats::<ReprConv<proto::database::Db>>(rng);
    test_encode_all_formats::<ReprConv<proto::database::Postgres>>(rng);
    test_encode_all_formats::<ReprConv<proto::eth::Eth>>(rng);
    test_encode_all_formats::<ReprConv<proto::prover::ProofCompressor>>(rng);
    test_encode_all_formats::<ReprConv<proto::prover::Prover>>(rng);
    test_encode_all_formats::<ReprConv<proto::prover::ProverGateway>>(rng);
    test_encode_all_formats::<ReprConv<proto::prover::ProverGroup>>(rng);
    test_encode_all_formats::<ReprConv<proto::prover::WitnessGenerator>>(rng);
    test_encode_all_formats::<ReprConv<proto::prover::WitnessVectorGenerator>>(rng);
    test_encode_all_formats::<ReprConv<proto::house_keeper::HouseKeeper>>(rng);
    test_encode_all_formats::<ReprConv<proto::object_store::ObjectStore>>(rng);
    test_encode_all_formats::<ReprConv<proto::prover::ProofDataHandler>>(rng);
    test_encode_all_formats::<ReprConv<proto::snapshot_creator::SnapshotsCreator>>(rng);
    test_encode_all_formats::<ReprConv<proto::observability::Observability>>(rng);
}

use std::{path::PathBuf, str::FromStr};

use zksync_protobuf::testonly::{test_encode_all_formats, ReprConv};

use crate::{decode_yaml_repr, proto};

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
    test_encode_all_formats::<ReprConv<proto::consensus::WeightedValidator>>(rng);
    test_encode_all_formats::<ReprConv<proto::consensus::GenesisSpec>>(rng);
    test_encode_all_formats::<ReprConv<proto::consensus::Config>>(rng);
    test_encode_all_formats::<ReprConv<proto::secrets::ConsensusSecrets>>(rng);
    test_encode_all_formats::<ReprConv<proto::secrets::Secrets>>(rng);
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

#[test]
fn verify_file_parsing() {
    let base_path = PathBuf::from_str("../../../etc/env/file_based/").unwrap();
    decode_yaml_repr::<proto::general::GeneralConfig>(&base_path.join("general.yaml"), true)
        .unwrap();
    // It's allowed to have unknown fields in wallets, e.g. we keep private key for fee account
    decode_yaml_repr::<proto::wallets::Wallets>(&base_path.join("wallets.yaml"), false).unwrap();
    decode_yaml_repr::<proto::genesis::Genesis>(&base_path.join("genesis.yaml"), true).unwrap();
    decode_yaml_repr::<proto::contracts::Contracts>(&base_path.join("contracts.yaml"), true)
        .unwrap();
    decode_yaml_repr::<proto::secrets::Secrets>(&base_path.join("secrets.yaml"), true).unwrap();
}

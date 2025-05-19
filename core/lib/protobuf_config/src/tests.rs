use std::{path::PathBuf, str::FromStr};

use zksync_protobuf::testonly::{test_encode_all_formats, ReprConv};

use crate::{proto, read_yaml_repr};

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
    test_encode_all_formats::<ReprConv<proto::consensus::RpcConfig>>(rng);
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
    test_encode_all_formats::<ReprConv<proto::prover::WitnessGenerator>>(rng);
    test_encode_all_formats::<ReprConv<proto::house_keeper::HouseKeeper>>(rng);
    test_encode_all_formats::<ReprConv<proto::object_store::ObjectStore>>(rng);
    test_encode_all_formats::<ReprConv<proto::prover::ProofDataHandler>>(rng);
    test_encode_all_formats::<ReprConv<proto::snapshot_creator::SnapshotsCreator>>(rng);
    test_encode_all_formats::<ReprConv<proto::observability::Observability>>(rng);
    test_encode_all_formats::<ReprConv<proto::wallets::Wallets>>(rng);
    test_encode_all_formats::<ReprConv<proto::genesis::Genesis>>(rng);
    test_encode_all_formats::<ReprConv<proto::en::ExternalNode>>(rng);
    test_encode_all_formats::<ReprConv<proto::da_client::DataAvailabilityClient>>(rng);
    test_encode_all_formats::<ReprConv<proto::da_dispatcher::DataAvailabilityDispatcher>>(rng);
    test_encode_all_formats::<ReprConv<proto::vm_runner::ProtectiveReadsWriter>>(rng);
    test_encode_all_formats::<ReprConv<proto::vm_runner::BasicWitnessInputProducer>>(rng);
    test_encode_all_formats::<ReprConv<proto::commitment_generator::CommitmentGenerator>>(rng);
    test_encode_all_formats::<ReprConv<proto::snapshot_recovery::Postgres>>(rng);
    test_encode_all_formats::<ReprConv<proto::snapshot_recovery::SnapshotRecovery>>(rng);
    test_encode_all_formats::<ReprConv<proto::pruning::Pruning>>(rng);
    test_encode_all_formats::<ReprConv<proto::base_token_adjuster::BaseTokenAdjuster>>(rng);
    test_encode_all_formats::<ReprConv<proto::external_price_api_client::ExternalPriceApiClient>>(
        rng,
    );
    test_encode_all_formats::<ReprConv<proto::general::GeneralConfig>>(rng);
}

#[test]
fn verify_file_parsing() {
    let base_path = PathBuf::from_str("../../../etc/env/file_based/").unwrap();
    read_yaml_repr::<proto::general::GeneralConfig>(&base_path.join("general.yaml"), false)
        .unwrap();
    // It's allowed to have unknown fields in wallets, e.g. we keep private key for fee account
    read_yaml_repr::<proto::wallets::Wallets>(&base_path.join("wallets.yaml"), false).unwrap();
    read_yaml_repr::<proto::genesis::Genesis>(&base_path.join("genesis.yaml"), true).unwrap();
    read_yaml_repr::<proto::contracts::Contracts>(&base_path.join("contracts.yaml"), true).unwrap();
    read_yaml_repr::<proto::secrets::Secrets>(&base_path.join("secrets.yaml"), true).unwrap();
    read_yaml_repr::<proto::en::ExternalNode>(&base_path.join("external_node.yaml"), true).unwrap();
}

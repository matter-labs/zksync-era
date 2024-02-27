use pretty_assertions::assert_eq;
use rand::Rng;
use testonly::{Gen, RandomConfig};
use zksync_config::testonly;

use crate::{proto, proto::FeeModelVersion::V1, repr::ProtoRepr};

fn encode<P: ProtoRepr>(msg: &P::Type) -> Vec<u8> {
    let msg = P::build(msg);
    zksync_protobuf::canonical_raw(&msg.encode_to_vec(), &msg.descriptor()).unwrap()
}

fn decode<P: ProtoRepr>(bytes: &[u8]) -> anyhow::Result<P::Type> {
    P::read(&P::decode(bytes)?)
}

fn encode_json<P: ProtoRepr>(msg: &P::Type) -> String {
    let mut s = serde_json::Serializer::pretty(vec![]);
    zksync_protobuf::serde::serialize_proto(&P::build(msg), &mut s).unwrap();
    String::from_utf8(s.into_inner()).unwrap()
}

fn decode_json<P: ProtoRepr>(json: &str) -> anyhow::Result<P::Type> {
    let mut d = serde_json::Deserializer::from_str(json);
    P::read(&zksync_protobuf::serde::deserialize_proto(&mut d)?)
}

#[track_caller]
fn encode_decode<P: ProtoRepr>(rng: &mut impl Rng)
where
    P::Type: PartialEq + std::fmt::Debug + testonly::RandomConfig,
{
    for required_only in [false, true] {
        let want: P::Type = testonly::Gen {
            rng,
            required_only,
            decimal_fractions: false,
        }
        .gen();
        let got = decode::<P>(&encode::<P>(&want)).unwrap();
        assert_eq!(&want, &got, "binary encoding");

        let want: P::Type = testonly::Gen {
            rng,
            required_only,
            decimal_fractions: true,
        }
        .gen();
        let got = decode_json::<P>(&encode_json::<P>(&want)).unwrap();
        assert_eq!(&want, &got, "json encoding");
    }
}

/// Tests config <-> proto (boilerplate) conversions.
#[test]
fn test_encoding() {
    let rng = &mut rand::thread_rng();
    encode_decode::<proto::Alerts>(rng);
    encode_decode::<proto::Web3JsonRpc>(rng);
    encode_decode::<proto::ContractVerificationApi>(rng);
    encode_decode::<proto::HealthCheck>(rng);
    encode_decode::<proto::MerkleTreeApi>(rng);
    encode_decode::<proto::Api>(rng);
    encode_decode::<proto::Prometheus>(rng);
    encode_decode::<proto::EthNetwork>(rng);
    encode_decode::<proto::StateKeeper>(rng);
    encode_decode::<proto::OperationsManager>(rng);
    encode_decode::<proto::Mempool>(rng);
    encode_decode::<proto::CircuitBreaker>(rng);
    encode_decode::<proto::ContractVerifier>(rng);
    encode_decode::<proto::Contracts>(rng);
    encode_decode::<proto::MerkleTree>(rng);
    encode_decode::<proto::Db>(rng);
    encode_decode::<proto::Postgres>(rng);
    encode_decode::<proto::EthClient>(rng);
    encode_decode::<proto::EthSender>(rng);
    encode_decode::<proto::Sender>(rng);
    encode_decode::<proto::GasAdjuster>(rng);
    encode_decode::<proto::EthWatch>(rng);
    encode_decode::<proto::FriProofCompressor>(rng);
    encode_decode::<proto::FriProofCompressor>(rng);
    encode_decode::<proto::FriProver>(rng);
    encode_decode::<proto::FriProverGateway>(rng);
    encode_decode::<proto::CircuitIdRoundTuple>(rng);
    encode_decode::<proto::FriProverGroup>(rng);
    encode_decode::<proto::FriWitnessGenerator>(rng);
    encode_decode::<proto::FriWitnessVectorGenerator>(rng);
    encode_decode::<proto::HouseKeeper>(rng);
    encode_decode::<proto::ObjectStore>(rng);
    encode_decode::<proto::ProofDataHandler>(rng);
    encode_decode::<proto::SnapshotsCreator>(rng);
    encode_decode::<proto::WitnessGenerator>(rng);
}

#[test]
fn test_encoding_state_keeper_mode() {
    // Test Rollup mode
    let mode_rollup = proto::L1BatchCommitDataGeneratorMode::Rollup;
    encode_decode_mode::<proto::StateKeeper>(&mode_rollup);

    // Test Validium mode
    let mode_validium = proto::L1BatchCommitDataGeneratorMode::Validium;
    encode_decode_mode::<proto::StateKeeper>(&mode_validium);
}

#[track_caller]
fn encode_decode_mode<P: ProtoRepr>(mode: &proto::L1BatchCommitDataGeneratorMode)
where
    P::Type: PartialEq + std::fmt::Debug + testonly::RandomConfig,
{
    for required_only in [false, true] {
        // Manually set the L1BatchCommitDataGeneratorMode
        let mut state_keeper = generate_mode(mode);
        dbg!(state_keeper);

        let rng = &mut rand::thread_rng();
        let want: P::Type = testonly::Gen {
            rng,
            required_only,
            decimal_fractions: false,
        }
        .gen();
        dbg!(want);
        assert_eq!(1, 2);
        //let got = decode::<P>(&encode::<P>(&want)).unwrap();

        //     let got = decode::<P>(&encode::<P>(&state_keeper)).unwrap();
        //     assert_eq!(&state_keeper, &got, "binary encoding");

        //     let got = decode_json::<P>(&encode_json::<P>(&state_keeper)).unwrap();
        //     assert_eq!(&state_keeper, &got, "json encoding");
        //
    }
}

fn generate_mode(mode: &proto::L1BatchCommitDataGeneratorMode) -> proto::StateKeeper {
    proto::StateKeeper {
        transaction_slots: Some(10926497003267881492),
        block_commit_deadline_ms: Some(10947007445086036797),
        miniblock_commit_deadline_ms: Some(9103314936350460727),
        miniblock_seal_queue_capacity: Some(6906530626745786909),
        max_single_tx_gas: Some(2785542050),
        max_allowed_l2_tx_gas_limit: Some(1183074970),
        reject_tx_at_geometry_percentage: Some(0.5296344133371791),
        reject_tx_at_eth_params_percentage: Some(0.8773607433255183),
        reject_tx_at_gas_percentage: Some(0.5289453936748325),
        close_block_at_geometry_percentage: Some(0.6479326814884391),
        close_block_at_eth_params_percentage: Some(0.21958980771195413),
        close_block_at_gas_percentage: Some(0.5229693273318018),
        fee_account_addr: Some(
            [
                153, 251, 1, 240, 243, 248, 139, 100, 180, 119, 34, 199, 146, 251, 165, 177, 250,
                52, 209, 52,
            ]
            .into(),
        ),
        minimal_l2_gas_price: Some(2703211627494097776),
        compute_overhead_part: Some(0.6857318990462495),
        pubdata_overhead_part: Some(0.12953109152591813),
        batch_overhead_l1_gas: Some(15518551973209220528),
        max_gas_per_batch: Some(15859976163468884008),
        max_pubdata_per_batch: Some(11012258501587398219),
        fee_model_version: Some(V1.into()),
        validation_computational_gas_limit: Some(2619723581),
        save_call_traces: Some(true),
        virtual_blocks_interval: Some(1113108050),
        virtual_blocks_per_miniblock: Some(756446353),
        upload_witness_inputs_to_gcs: Some(true),
        enum_index_migration_chunk_size: Some(3905631822449257186),
        l1_batch_commit_data_generator_mode: Some(mode.clone().into()),
    }
}

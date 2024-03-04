use pretty_assertions::assert_eq;
use rand::Rng;
use zksync_config::testonly;
use zksync_protobuf::repr::ProtoRepr;

use crate::proto;

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
    encode_decode::<proto::alerts::Alerts>(rng);
    encode_decode::<proto::api::Web3JsonRpc>(rng);
    encode_decode::<proto::api::ContractVerificationApi>(rng);
    encode_decode::<proto::api::HealthCheck>(rng);
    encode_decode::<proto::api::MerkleTreeApi>(rng);
    encode_decode::<proto::api::Api>(rng);
    encode_decode::<proto::utils::Prometheus>(rng);
    encode_decode::<proto::chain::EthNetwork>(rng);
    encode_decode::<proto::chain::StateKeeper>(rng);
    encode_decode::<proto::chain::OperationsManager>(rng);
    encode_decode::<proto::chain::Mempool>(rng);
    encode_decode::<proto::chain::CircuitBreaker>(rng);
    encode_decode::<proto::contract_verifier::ContractVerifier>(rng);
    encode_decode::<proto::contracts::Contracts>(rng);
    encode_decode::<proto::database::MerkleTree>(rng);
    encode_decode::<proto::database::Db>(rng);
    encode_decode::<proto::database::Postgres>(rng);
    encode_decode::<proto::eth_client::EthClient>(rng);
    encode_decode::<proto::eth_sender::EthSender>(rng);
    encode_decode::<proto::eth_sender::Sender>(rng);
    encode_decode::<proto::eth_sender::GasAdjuster>(rng);
    encode_decode::<proto::eth_watch::EthWatch>(rng);
    encode_decode::<proto::fri_proof_compressor::FriProofCompressor>(rng);
    encode_decode::<proto::fri_prover::FriProver>(rng);
    encode_decode::<proto::fri_prover_gateway::FriProverGateway>(rng);
    encode_decode::<proto::fri_prover_group::CircuitIdRoundTuple>(rng);
    encode_decode::<proto::fri_prover_group::FriProverGroup>(rng);
    encode_decode::<proto::fri_witness_generator::FriWitnessGenerator>(rng);
    encode_decode::<proto::fri_witness_vector_generator::FriWitnessVectorGenerator>(rng);
    encode_decode::<proto::house_keeper::HouseKeeper>(rng);
    encode_decode::<proto::object_store::ObjectStore>(rng);
    encode_decode::<proto::proof_data_handler::ProofDataHandler>(rng);
    encode_decode::<proto::snapshot_creator::SnapshotsCreator>(rng);
    encode_decode::<proto::witness_generator::WitnessGenerator>(rng);
    encode_decode::<proto::kzg::Kzg>(rng);
    encode_decode::<proto::observability::Observability>(rng);
}

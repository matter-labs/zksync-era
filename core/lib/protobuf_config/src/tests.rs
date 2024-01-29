use pretty_assertions::assert_eq;
use rand::Rng;
use zksync_config::testonly;

use crate::{proto, repr::ProtoRepr};

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

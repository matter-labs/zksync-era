use pretty_assertions::assert_eq;
use rand::Rng;
use zksync_config::testonly;
use zksync_protobuf::{
    build::{prost::Message as _, prost_reflect::ReflectMessage},
    repr::ProtoRepr,
    ProtoFmt,
};

pub trait ProtoConv {
    type Type;
    type Proto: ReflectMessage + Default;
    fn read(r: &Self::Proto) -> anyhow::Result<Self::Type>;
    fn build(this: &Self::Type) -> Self::Proto;
}

pub fn encode<X: ProtoConv>(msg: &X::Type) -> Vec<u8> {
    let msg = X::build(msg);
    zksync_protobuf::canonical_raw(&msg.encode_to_vec(), &msg.descriptor()).unwrap()
}

pub fn decode<X: ProtoConv>(bytes: &[u8]) -> anyhow::Result<X::Type> {
    X::read(&X::Proto::decode(bytes)?)
}

pub fn encode_json<X: ProtoConv>(msg: &X::Type) -> String {
    let mut s = serde_json::Serializer::pretty(vec![]);
    zksync_protobuf::serde::serialize_proto(&X::build(msg), &mut s).unwrap();
    String::from_utf8(s.into_inner()).unwrap()
}

pub fn decode_json<X: ProtoConv>(json: &str) -> anyhow::Result<X::Type> {
    let mut d = serde_json::Deserializer::from_str(json);
    X::read(&zksync_protobuf::serde::deserialize_proto(&mut d)?)
}

pub fn encode_yaml<X: ProtoConv>(msg: &X::Type) -> String {
    let mut s = serde_yaml::Serializer::new(vec![]);
    zksync_protobuf::serde::serialize_proto(&X::build(msg), &mut s).unwrap();
    String::from_utf8(s.into_inner().unwrap()).unwrap()
}

fn decode_yaml<X: ProtoConv>(yaml: &str) -> anyhow::Result<X::Type> {
    let d = serde_yaml::Deserializer::from_str(yaml);
    X::read(&zksync_protobuf::serde::deserialize_proto(d)?)
}

pub struct ReprConv<P: ProtoRepr>(std::marker::PhantomData<P>);
pub struct FmtConv<T: ProtoFmt>(std::marker::PhantomData<T>);

impl<T: ProtoFmt> ProtoConv for FmtConv<T> {
    type Type = T;
    type Proto = T::Proto;
    fn read(r: &T::Proto) -> anyhow::Result<T> {
        ProtoFmt::read(r)
    }
    fn build(this: &T) -> T::Proto {
        ProtoFmt::build(this)
    }
}

impl<P: ProtoRepr> ProtoConv for ReprConv<P> {
    type Type = P::Type;
    type Proto = P;
    fn read(r: &P) -> anyhow::Result<P::Type> {
        ProtoRepr::read(r)
    }
    fn build(this: &P::Type) -> P {
        ProtoRepr::build(this)
    }
}

pub fn encode_decode<X: ProtoConv>(rng: &mut impl Rng)
where
    X::Type: std::fmt::Debug + testonly::RandomConfig + PartialEq,
{
    for required_only in [false, true] {
        let want: X::Type = testonly::Gen {
            rng,
            required_only,
            decimal_fractions: false,
        }
        .gen();
        let got = decode::<X>(&encode::<X>(&want)).unwrap();
        assert_eq!(&want, &got, "binary encoding");
        let got = decode_yaml::<X>(&encode_yaml::<X>(&want)).unwrap();
        assert_eq!(&want, &got, "yaml encoding");

        let want: X::Type = testonly::Gen {
            rng,
            required_only,
            decimal_fractions: true,
        }
        .gen();
        let got = decode_json::<X>(&encode_json::<X>(&want)).unwrap();
        assert_eq!(&want, &got, "json encoding");
    }
}

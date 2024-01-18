//! Generates rust code from protobufs.
fn main() {
    zksync_protobuf_build::Config {
        input_root: "src/models/proto".into(),
        proto_root: "zksync/dal".into(),
        dependencies: vec![],
        protobuf_crate: "::zksync_protobuf".parse().unwrap(),
        is_public: true,
    }
    .generate()
    .expect("generate()");
}

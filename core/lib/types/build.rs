//! Generates rust code from protobufs.
fn main() {
    zksync_protobuf_build::Config {
        input_root: "src/proto".into(),
        proto_root: "zksync/types".into(),
        dependencies: vec![],
        protobuf_crate: "::zksync_protobuf".parse().unwrap(),
        is_public: false,
    }
    .generate()
    .expect("generate()");
}

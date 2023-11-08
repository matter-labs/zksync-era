//! Generates rust code from protobufs.
fn main() {
    zksync_protobuf::build::Config {
        input_root: "src/configs/proto".into(),
        proto_root: "zksync/config/configs".into(),
        dependencies: vec![],
        protobuf_crate: "::zksync_protobuf".into(),
    }
    .generate()
    .expect("generate()");
}

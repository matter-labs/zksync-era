//! Generates rust code from protobufs.
fn main() {
    zksync_protobuf::build::Config {
        input_root: "src/proto".into(),
        proto_root: "zksync/types".into(),
        dependencies: vec![(
            "::zksync_consensus_roles::proto".parse().unwrap(),
            &zksync_consensus_roles::proto::DESCRIPTOR,
        )],
        protobuf_crate: "::zksync_protobuf".parse().unwrap(),
    }
    .generate()
    .expect("generate()");
}

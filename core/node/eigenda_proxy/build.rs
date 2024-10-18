fn main() {
    tonic_build::configure()
        .build_server(false)
        .out_dir("src/")
        .compile_protos(&["proto/disperser/disperser.proto"], &["proto"])
        .unwrap();
}

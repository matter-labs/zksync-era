{ pkgs
, lib
, fetchFromGitHub
, inputs
, ...
}:
let
  src = fetchFromGitHub {
    owner = "matter-labs";
    repo = "foundry-zksync";
    tag = "foundry-zksync-v0.0.11";
    hash = "sha256-NSVzqv4UsAfW6i4b+Rj7ccFM66fbGlpsTJ1qPIgn8F0=";
  };

  toolchain = pkgs.rust-bin.fromRustupToolchainFile "${src}/rust-toolchain";

  craneLib = (inputs.crane.mkLib pkgs).overrideToolchain toolchain;

  rustPlatform = pkgs.makeRustPlatform {
    cargo = toolchain;
    rustc = toolchain;
  };
in
craneLib.buildPackage {
  # Some crates download stuff from the network while compiling!!!!
  # Allows derivation to access network
  #
  # Users of this package must set options to indicate that the sandbox conditions can be relaxed for this package.
  # These are:
  # - When used in a flake, set the flake's config with this line: nixConfig.sandbox = false;
  # - From the command line with nix <command>, add one of these options:
  #   - --option sandbox false
  #   - --no-sandbox
  __noChroot = true;

  pname = src.repo;
  version = src.tag;
  inherit src;

  doCheck = false;

  nativeBuildInputs = with pkgs;[
    pkg-config
    rustPlatform.bindgenHook
  ] ++ lib.optionals stdenv.hostPlatform.isDarwin [ darwin.DarwinTools ];

  buildInputs = with pkgs;[
    libusb1.dev
    libclang.dev
    openssl.dev
    lz4.dev
    bzip2.dev
    rocksdb_8_3
    snappy.dev
  ] ++ lib.optionals stdenv.hostPlatform.isDarwin [ darwin.apple_sdk.frameworks.AppKit ];

  env = {
    OPENSSL_NO_VENDOR = "1";
    ROCKSDB_LIB_DIR = "${pkgs.rocksdb_8_3.out}/lib";
    ROCKSDB_INCLUDE_DIR = "${pkgs.rocksdb_8_3.out}/include";
    SNAPPY_LIB_DIR = "${pkgs.snappy.out}/lib";
  };
}

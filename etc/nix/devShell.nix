{ pkgs
, tee_prover
, coreCommonArgs
, inputs
, ...
}:
let
  toolchain = pkgs.rust-bin.fromRustupToolchainFile (inputs.src + "/rust-toolchain");

  toolchain_with_src = (toolchain.override {
    extensions = [ "rustfmt" "clippy" "rust-src" ];
  });
in
pkgs.mkShell rec {
  inputsFrom = [ tee_prover ];
  packages = [ ];

  inherit (coreCommonArgs) hardeningEnable;

  shellHook = ''
    export ZKSYNC_HOME=$PWD
    export PATH=$ZKSYNC_HOME/bin:$PATH
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:$ZK_NIX_LD_LIBRARY_PATH"
  '';

  env = coreCommonArgs.env // {
    RUST_SRC_PATH = "${toolchain_with_src}/lib/rustlib/src/rust/library";
    ZK_NIX_LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath coreCommonArgs.buildInputs;
  };
}


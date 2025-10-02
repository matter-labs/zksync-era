{ pkgs
, tee_prover
, coreCommonArgs
, inputs
, ...
}:
let
  toolchain = pkgs.rust-bin.fromRustupToolchainFile (inputs.src + "/rust-toolchain");

  toolchain_with_src = toolchain.override {
    extensions = [ "rustfmt" "clippy" "rust-src" ];
  };
in
pkgs.mkShell {
  inputsFrom = [ tee_prover ];
  packages = [ ];

  inherit (coreCommonArgs) hardeningEnable;

  shellHook = ''
    export ZKSYNC_HOME=$PWD
    export PATH=$ZKSYNC_HOME/bin:$PATH
    if [ "x$NIX_LD" = "x" ]; then
      export NIX_LD=$(<${pkgs.clangStdenv.cc}/nix-support/dynamic-linker)
    fi
    if [ "x$NIX_LD_LIBRARY_PATH" = "x" ]; then
      export NIX_LD_LIBRARY_PATH="$ZK_NIX_LD_LIBRARY_PATH"
    else
      export NIX_LD_LIBRARY_PATH="$NIX_LD_LIBRARY_PATH:$ZK_NIX_LD_LIBRARY_PATH"
    fi
  '';

  env = coreCommonArgs.env // {
    RUST_SRC_PATH = "${toolchain_with_src}/lib/rustlib/src/rust/library";
    ZK_NIX_LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath coreCommonArgs.buildInputs;
  };
}


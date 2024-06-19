###################################################################################################
#
# To build the rust components with this flake, run:
# $ nix build .#cargoDeps
# set `cargoHash` below to the result of the build
# then
# $ nix build .#zksync_server
# or
# $ nix build .#zksync_server.contract_verifier
# $ nix build .#zksync_server.external_node
# $ nix build .#zksync_server.server
# $ nix build .#zksync_server.snapshots_creator
# $ nix build .#zksync_server.block_reverter
#
# To enter the development shell, run:
# $ nix develop
#
# To vendor the dependencies manually, run:
# $ nix shell .#cargo-vendor -c cargo vendor --no-merge-sources
#
###################################################################################################
{
  description = "ZKsync-era";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.05";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };
  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        ###########################################################################################
        # This changes every time `Cargo.lock` changes. Set to `null` to force re-vendoring
        cargoHash = null;
        # cargoHash = "sha256-LloF3jrvFkOlZ2lQXB+/sFthfJQLLu8BvHBE88gRvFc=";
        ###########################################################################################
        officialRelease = false;

        versionSuffix =
          if officialRelease
          then ""
          else "pre${builtins.substring 0 8 (self.lastModifiedDate or self.lastModified or "19700101")}_${self.shortRev or "dirty"}";

        pkgs = import nixpkgs { inherit system; overlays = [ rust-overlay.overlays.default ]; };

        # patched version of cargo to support `cargo vendor` for vendoring dependencies
        # see https://github.com/matter-labs/zksync-era/issues/1086
        # used as `cargo vendor --no-merge-sources`
        cargo-vendor = pkgs.rustPlatform.buildRustPackage {
          pname = "cargo-vendor";
          version = "0.78.0";
          src = pkgs.fetchFromGitHub {
            owner = "haraldh";
            repo = "cargo";
            rev = "3ee1557d2bd95ca9d0224c5dbf1d1e2d67186455";
            hash = "sha256-A8xrOG+NmF8dQ7tA9I2vJSNHlYxsH44ZRXdptLblCXk=";
          };
          doCheck = false;
          cargoHash = "sha256-LtuNtdoX+FF/bG5LQc+L2HkFmgCtw5xM/m0/0ShlX2s=";
          nativeBuildInputs = [
            pkgs.pkg-config
            pkgs.rustPlatform.bindgenHook
          ];
          buildInputs = [
            pkgs.openssl
          ];
        };

        # custom import-cargo-lock to import Cargo.lock file and vendor dependencies
        # see https://github.com/matter-labs/zksync-era/issues/1086
        import-cargo-lock = { lib, cacert, runCommand }: { src, cargoHash ? null }:
          runCommand "import-cargo-lock"
            {
              inherit src;
              nativeBuildInputs = [ cargo-vendor cacert ];
              preferLocalBuild = true;
              outputHashMode = "recursive";
              outputHashAlgo = "sha256";
              outputHash = if cargoHash != null then cargoHash else lib.fakeSha256;
            }
            ''
              mkdir -p $out/.cargo
              mkdir -p $out/cargo-vendor-dir

              HOME=$(pwd)
              pushd ${src}
              HOME=$HOME cargo vendor --no-merge-sources $out/cargo-vendor-dir > $out/.cargo/config
              sed -i -e "s#$out#import-cargo-lock#g" $out/.cargo/config
              cp $(pwd)/Cargo.lock $out/Cargo.lock
              popd
            ''
        ;
        cargoDeps = pkgs.buildPackages.callPackage import-cargo-lock { } { inherit src; inherit cargoHash; };

        rustVersion = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain;

        stdenv = pkgs.stdenvAdapters.useMoldLinker pkgs.clangStdenv;

        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustVersion;
          rustc = rustVersion;
          inherit stdenv;
        };
        zksync_server_cargoToml = builtins.fromTOML (builtins.readFile ./core/bin/zksync_server/Cargo.toml);

        hardeningEnable = [ "fortify3" "pie" "relro" ];

        src = with pkgs.lib.fileset; toSource {
          root = ./.;
          fileset = unions [
            ./Cargo.lock
            ./Cargo.toml
            ./core
            ./prover
            ./.github/release-please/manifest.json
          ];
        };

        zksync_server = with pkgs; stdenv.mkDerivation {
          pname = "zksync";
          version = zksync_server_cargoToml.package.version + versionSuffix;

          updateAutotoolsGnuConfigScriptsPhase = ":";

          nativeBuildInputs = [
            pkg-config
            rustPlatform.bindgenHook
            rustPlatform.cargoSetupHook
            rustPlatform.cargoBuildHook
            rustPlatform.cargoInstallHook
          ];

          buildInputs = [
            libclang
            openssl
            snappy.dev
            lz4.dev
            bzip2.dev
          ];

          inherit src;
          cargoBuildFlags = "--all";
          cargoBuildType = "release";

          inherit cargoDeps;

          inherit hardeningEnable;

          outputs = [
            "out"
            "contract_verifier"
            "external_node"
            "server"
            "snapshots_creator"
            "block_reverter"
          ];

          postInstall = ''
            mkdir -p $out/nix-support
            for i in $outputs; do
              [[ $i == "out" ]] && continue
              mkdir -p "''${!i}/bin"
              echo "''${!i}" >> $out/nix-support/propagated-user-env-packages
              if [[ -e "$out/bin/zksync_$i" ]]; then
                mv "$out/bin/zksync_$i" "''${!i}/bin"
              else
                mv "$out/bin/$i" "''${!i}/bin"
              fi
            done

            mkdir -p $external_node/nix-support
            echo "block_reverter" >> $external_node/nix-support/propagated-user-env-packages

            mv $out/bin/merkle_tree_consistency_checker $server/bin
            mkdir -p $server/nix-support
            echo "block_reverter" >> $server/nix-support/propagated-user-env-packages
          '';
        };
      in
      {
        formatter = pkgs.nixpkgs-fmt;

        packages = {
          inherit zksync_server;
          default = zksync_server;
          inherit cargo-vendor;
          inherit cargoDeps;
        };

        devShells = with pkgs; {
          default = pkgs.mkShell.override { inherit stdenv; } {
            inputsFrom = [ zksync_server ];

            packages = [
              docker-compose
              nodejs
              yarn
              axel
              postgresql
              python3
              solc
              sqlx-cli
              mold
            ];

            inherit hardeningEnable;

            shellHook = ''
              export ZKSYNC_HOME=$PWD
              export PATH=$ZKSYNC_HOME/bin:$PATH
              export RUSTFLAGS='-C link-arg=-fuse-ld=${pkgs.mold}/bin/mold'
              export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="clang"

              if [ "x$NIX_LD" = "x" ]; then
                export NIX_LD="$(<${clangStdenv.cc}/nix-support/dynamic-linker)"
              fi
              if [ "x$NIX_LD_LIBRARY_PATH" = "x" ]; then
                export NIX_LD_LIBRARY_PATH="$ZK_NIX_LD_LIBRARY_PATH"
              else
                export NIX_LD_LIBRARY_PATH="$NIX_LD_LIBRARY_PATH:$ZK_NIX_LD_LIBRARY_PATH"
              fi
            '';

            ZK_NIX_LD_LIBRARY_PATH = lib.makeLibraryPath [ ];
          };
        };
      });
}


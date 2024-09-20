###################################################################################################
#
# see `README.md` in `etc/nix`
#
###################################################################################################
{
  description = "ZKsync-era";

  nixConfig = {
    extra-substituters = [ "https://attic.teepot.org/tee-pot" ];
    extra-trusted-public-keys = [ "tee-pot:SS6HcrpG87S1M6HZGPsfo7d1xJccCGev7/tXc5+I4jg=" ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.05";
    teepot-flake.url = "github:matter-labs/teepot";
    nixsgx-flake.url = "github:matter-labs/nixsgx";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane = {
      url = "github:ipetkov/crane?tag=v0.17.3";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, teepot-flake, nixsgx-flake, flake-utils, rust-overlay, crane }:
    let
      officialRelease = false;
      hardeningEnable = [ "fortify3" "pie" "relro" ];

      out = system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [
              rust-overlay.overlays.default
              nixsgx-flake.overlays.default
              teepot-flake.overlays.default
            ];
          };

          appliedOverlay = self.overlays.default pkgs pkgs;
        in
        {
          formatter = pkgs.nixpkgs-fmt;

          packages = {
            # to ease potential cross-compilation, the overlay is used
            inherit (appliedOverlay.zksync-era) zksync tee_prover container-tee-prover-azure container-tee-prover-dcap;
            default = appliedOverlay.zksync-era.zksync;
          };

          devShells.default = appliedOverlay.zksync-era.devShell;
        };
    in
    flake-utils.lib.eachDefaultSystem out // {
      overlays.default = final: prev:
        # to ease potential cross-compilation, the overlay is used
        let
          pkgs = final;

          rustVersion = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain;

          rustPlatform = pkgs.makeRustPlatform {
            cargo = rustVersion;
            rustc = rustVersion;
          };

          craneLib = (crane.mkLib pkgs).overrideToolchain rustVersion;

          commonArgs = {
            nativeBuildInputs = with pkgs;[
              pkg-config
              rustPlatform.bindgenHook
            ];

            buildInputs = with pkgs;[
              libclang.dev
              openssl.dev
              snappy.dev
              lz4.dev
              bzip2.dev
              rocksdb
              snappy.dev
            ];

            src = with pkgs.lib.fileset; toSource {
              root = ./.;
              fileset = unions [
                ./Cargo.lock
                ./Cargo.toml
                ./core
                ./prover
                ./zk_toolbox
                ./.github/release-please/manifest.json
              ];
            };

            env = {
              OPENSSL_NO_VENDOR = "1";
              ROCKSDB_LIB_DIR = "${pkgs.rocksdb.out}/lib";
              SNAPPY_LIB_DIR = "${pkgs.snappy.out}/lib";
              NIX_OUTPATH_USED_AS_RANDOM_SEED = "aaaaaaaaaa";
            };

            doCheck = false;
            strictDeps = true;
            inherit hardeningEnable;
          };

          cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
            pname = "zksync-era-workspace";
          });
        in
        {
          zksync-era = rec {
            devShell = pkgs.callPackage ./etc/nix/devshell.nix {
              inherit zksync;
              inherit commonArgs;
            };

            zksync = pkgs.callPackage ./etc/nix/zksync.nix {
              inherit cargoArtifacts;
              inherit craneLib;
              inherit commonArgs;
            };
            tee_prover = pkgs.callPackage ./etc/nix/tee_prover.nix {
              inherit cargoArtifacts;
              inherit craneLib;
              inherit commonArgs;
            };

            container-tee-prover-azure = pkgs.callPackage ./etc/nix/container-tee_prover.nix {
              inherit tee_prover;
              isAzure = true;
              container-name = "zksync-tee-prover-azure";
            };
            container-tee-prover-dcap = pkgs.callPackage ./etc/nix/container-tee_prover.nix {
              inherit tee_prover;
              isAzure = false;
              container-name = "zksync-tee-prover-dcap";
            };
          };
        };
    };
}


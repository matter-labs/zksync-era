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
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";
    teepot-flake.url = "github:matter-labs/teepot";
    nixsgx-flake.url = "github:matter-labs/nixsgx";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane?tag=v0.20.0";
  };

  outputs = { self, nixpkgs, teepot-flake, nixsgx-flake, flake-utils, rust-overlay, crane } @ inputs:
    let
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
            inherit (appliedOverlay.zksync-era) zksync tee_prover zkstack foundry-zksync;
            default = appliedOverlay.zksync-era.tee_prover;
          } // (pkgs.lib.optionalAttrs (pkgs.stdenv.hostPlatform.isx86_64 && pkgs.stdenv.hostPlatform.isLinux) {
            inherit (appliedOverlay.zksync-era) container-tee-prover-azure container-tee-prover-dcap container-tee-prover-tdx;
          });

          devShells = {
            inherit (appliedOverlay.zksync-era) devShell devShellAll;
            default = appliedOverlay.zksync-era.devShell;
          };
        };
    in
    flake-utils.lib.eachDefaultSystem out // {
      overlays.default = final: prev:
        # to ease potential cross-compilation, the overlay is used
        let
          pkgs = final;

          toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain;

          rustPlatform = pkgs.makeRustPlatform {
            cargo = toolchain;
            rustc = toolchain;
          };

          craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

          coreCommonArgs = {
            nativeBuildInputs = with pkgs;[
              pkg-config
              rustPlatform.bindgenHook
            ];

            buildInputs = with pkgs;[
              libclang
              openssl
              snappy
              lz4
              bzip2
              snappy
              rocksdb_8_3
              postgresql
            ];

            src = with pkgs.lib.fileset; let root = ./core/.; in toSource {
              inherit root;
              fileset = unions [
                # Default files from crane (Rust and cargo files)
                (craneLib.fileset.commonCargoSources root)
                # proto files and friends
                (fileFilter (file: file.hasExt "proto" || file.hasExt "js" || file.hasExt "ts" || file.hasExt "map" || file.hasExt "json") root)
                (maybeMissing ./core/lib/dal/.)
              ];
            };

            env = {
              OPENSSL_NO_VENDOR = "1";
              ROCKSDB_LIB_DIR = "${pkgs.rocksdb_8_3.out}/lib";
              ROCKSDB_INCLUDE_DIR = "${pkgs.rocksdb_8_3.out}/include";
              SNAPPY_LIB_DIR = "${pkgs.snappy.out}/lib";
              NIX_OUTPATH_USED_AS_RANDOM_SEED = "aaaaaaaaaa";
            };

            doCheck = false;
            strictDeps = true;
            inherit hardeningEnable;
          };

          zkstackArgs = coreCommonArgs // {
            src = with pkgs.lib.fileset; let root = ./.; in toSource {
              inherit root;
              fileset = unions [
                # Default files from crane (Rust and cargo files)
                (craneLib.fileset.commonCargoSources root)
                # proto files and friends
                (fileFilter (file: file.hasExt "proto" || file.hasExt "js" || file.hasExt "ts" || file.hasExt "map" || file.hasExt "json") ./.)
                (maybeMissing ./core/lib/dal/.)
              ];
            };
          };
        in
        {
          zksync-era = pkgs.lib.makeScope pkgs.newScope (
            self: pkgs.lib.filesystem.packagesFromDirectoryRecursive {
              callPackage = package: params: self.callPackage package (params // {
                inherit craneLib;
                inherit coreCommonArgs;
                inherit zkstackArgs;
                inherit rustPlatform;
                inputs = inputs // { src = ./.; };
              });
              directory = ./etc/nix;
            }
          );
        };
    };
}


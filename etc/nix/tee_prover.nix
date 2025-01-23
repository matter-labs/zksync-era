{ craneLib
, coreCommonArgs
, ...
}:
let
  pname = "zksync_tee_prover";
  cargoExtraArgs = "--locked -p zksync_tee_prover";
in
craneLib.buildPackage (coreCommonArgs // {
  inherit pname;
  version = (builtins.fromTOML (builtins.readFile ../../core/Cargo.toml)).workspace.package.version;
  inherit cargoExtraArgs;

  cargoArtifacts = craneLib.buildDepsOnly (coreCommonArgs // {
    inherit pname;
    inherit cargoExtraArgs;
  });

  postInstall = ''
    strip $out/bin/zksync_tee_prover
  '';

  # zksync-protobuf has store paths
  postPatch = ''
    mkdir -p "$TMPDIR/nix-vendor"
    cp -Lr "$cargoVendorDir" -T "$TMPDIR/nix-vendor"
    sed -i "s|$cargoVendorDir|$TMPDIR/nix-vendor/|g" "$TMPDIR/nix-vendor/config.toml"
    chmod -R +w "$TMPDIR/nix-vendor"
    cargoVendorDir="$TMPDIR/nix-vendor"
  '';
})

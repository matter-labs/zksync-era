{ craneLib
, commonArgs
}:
let
  pname = "zksync_tee_prover";
  cargoExtraArgs = "--locked -p zksync_tee_prover";
in
craneLib.buildPackage (commonArgs // {
  inherit pname;
  version = (builtins.fromTOML (builtins.readFile ../../core/Cargo.toml)).workspace.package.version;
  inherit cargoExtraArgs;

  cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
    inherit pname;
    inherit cargoExtraArgs;
  });

  postInstall = ''
    strip $out/bin/zksync_tee_prover
  '';
})

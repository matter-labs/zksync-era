{ cargoArtifacts
, craneLib
, versionSuffix
, commonArgs
}:
craneLib.buildPackage (commonArgs // {
  pname = "zksync";
  version = (builtins.fromTOML (builtins.readFile ../../core/bin/zksync_tee_prover/Cargo.toml)).package.version + versionSuffix;
  cargoExtraArgs = "--all";
  inherit cargoArtifacts;

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
})

{ craneLib
, coreCommonArgs
, zkstack
, foundry-zksync
, ...
}:
let
  cargoExtraArgs = "--locked --bin zksync_server --bin zksync_contract_verifier --bin zksync_external_node --bin snapshots_creator --bin block_reverter --bin merkle_tree_consistency_checker";
in
craneLib.buildPackage (coreCommonArgs // {
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

  pname = "zksync";
  version = (builtins.fromTOML (builtins.readFile (coreCommonArgs.src + "/Cargo.toml"))).workspace.package.version;
  inherit cargoExtraArgs;

  buildInputs = coreCommonArgs.buildInputs ++ [
    zkstack
    foundry-zksync
  ];

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

  # zksync-protobuf has store paths
  postPatch = ''
    mkdir -p "$TMPDIR/nix-vendor"
    cp -Lr "$cargoVendorDir" -T "$TMPDIR/nix-vendor"
    sed -i "s|$cargoVendorDir|$TMPDIR/nix-vendor/|g" "$TMPDIR/nix-vendor/config.toml"
    chmod -R +w "$TMPDIR/nix-vendor"
    cargoVendorDir="$TMPDIR/nix-vendor"
  '';
}
)


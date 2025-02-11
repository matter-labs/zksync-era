{ craneLib
, zkstackArgs
, ...
}:
craneLib.buildPackage (zkstackArgs // {
  # Some crates download stuff from the network while compiling!!!!
  # Allows derivation to access the network
  #
  # Users of this package must set options to indicate that the sandbox conditions can be relaxed for this package.
  # These are:
  # - In the appropriate nix.conf file (depends on multi vs single user nix installation), add the line: sandbox = relaxed
  # - When used in a flake, set the flake's config with this line: nixConfig.sandbox = "relaxed";
  # - Same as above, but disabling the sandbox completely: nixConfig.sandbox = false;
  # - From the command line with nix <command>, add one of these options:
  #   - --option sandbox relaxed
  #   - --option sandbox false
  #   - --no-sandbox
  #   - --relaxed-sandbox
  __noChroot = true;
  cargoToml = "${zkstackArgs.src}/zkstack_cli/Cargo.toml";
  cargoLock = "${zkstackArgs.src}/zkstack_cli/Cargo.lock";

  pname = "zkstack";

  cargoArtifacts = craneLib.buildDepsOnly (zkstackArgs // {
    pname = "zkstack-workspace";
    cargoToml = "${zkstackArgs.src}/zkstack_cli/Cargo.toml";
    cargoLock = "${zkstackArgs.src}/zkstack_cli/Cargo.lock";
    postUnpack = ''
      cd $sourceRoot/zkstack_cli
      sourceRoot="."
    '';
  });

  version = (builtins.fromTOML (builtins.readFile "${zkstackArgs.src}/zkstack_cli/Cargo.toml")).workspace.package.version;

  postUnpack = ''
    cd $sourceRoot/zkstack_cli
    sourceRoot="."
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

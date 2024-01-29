{
  description = "zkSync development shell";
  inputs = {
    stable.url = "github:NixOS/nixpkgs/nixos-23.11";
  };
  outputs = { self, stable }: {
    formatter.x86_64-linux = stable.legacyPackages.x86_64-linux.nixpkgs-fmt;
    devShells.x86_64-linux.default =
      with import stable { system = "x86_64-linux"; };
      pkgs.mkShell.override { stdenv = pkgs.stdenvAdapters.useMoldLinker pkgs.gccStdenv; } {
        name = "zkSync";
        src = ./.;
        buildInputs = [
          docker-compose
          nodejs
          yarn
          axel
          libclang
          openssl
          pkg-config
          postgresql
          python3
          solc
          sqlx-cli
          rustup
        ];

        # for RocksDB and other Rust bindgen libraries
        LIBCLANG_PATH = lib.makeLibraryPath [ libclang.lib ];
        BINDGEN_EXTRA_CLANG_ARGS = ''-I"${libclang.lib}/lib/clang/${builtins.elemAt (builtins.splitVersion libclang.version) 0}/include"'';

        shellHook = ''
          export ZKSYNC_HOME=$PWD
          export PATH=$ZKSYNC_HOME/bin:$PATH
        '';

        # hardhat solc requires ld-linux
        # Nixos has to fake it with nix-ld
        NIX_LD_LIBRARY_PATH = lib.makeLibraryPath [];
        NIX_LD = builtins.readFile "${stdenv.cc}/nix-support/dynamic-linker";
      };
  };
}

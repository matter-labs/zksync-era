{
    description = "zkSync development shell";
    inputs = {
        stable.url = "github:NixOS/nixpkgs/nixos-22.11";
    };
    outputs = {self, stable}: {
        packages.x86_64-linux.default =
        with import stable { system = "x86_64-linux"; };
        pkgs.mkShell {
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
            ];

            # for RocksDB and other Rust bindgen libraries
            LIBCLANG_PATH = lib.makeLibraryPath [ libclang.lib ];
            BINDGEN_EXTRA_CLANG_ARGS = ''-I"${libclang.lib}/lib/clang/${libclang.version}/include"'';

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

{ pkgs
, zksync_server
, commonArgs
}:
pkgs.mkShell {
  inputsFrom = [ zksync_server ];

  packages = with pkgs; [
    docker-compose
    nodejs
    yarn
    axel
    postgresql
    python3
    solc
    sqlx-cli
  ];

  inherit (commonArgs) env hardeningEnable;

  shellHook = ''
    export ZKSYNC_HOME=$PWD
    export PATH=$ZKSYNC_HOME/bin:$PATH

    if [ "x$NIX_LD" = "x" ]; then
      export NIX_LD=$(<${pkgs.clangStdenv.cc}/nix-support/dynamic-linker)
    fi
    if [ "x$NIX_LD_LIBRARY_PATH" = "x" ]; then
      export NIX_LD_LIBRARY_PATH="$ZK_NIX_LD_LIBRARY_PATH"
    else
      export NIX_LD_LIBRARY_PATH="$NIX_LD_LIBRARY_PATH:$ZK_NIX_LD_LIBRARY_PATH"
    fi
  '';

  ZK_NIX_LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [ ];
}


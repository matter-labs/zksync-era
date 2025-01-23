{ pkgs
, zksync
, zkstack
, devShell
, foundry-zksync
, ...
}: devShell.override {
  customInputsFrom = [ zksync zkstack ];

  customPackages = with pkgs; [
    docker-compose
    nodejs
    yarn
    axel
    postgresql
    python3
    solc
    sqlx-cli
    zkstack
    foundry-zksync
    nodePackages.prettier
  ];
}

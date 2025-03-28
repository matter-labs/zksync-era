{ pkgs
, zksync
, zkstack
, devShell
, foundry-zksync
, ...
}:
let
  newshell = (pkgs.mkShell {
    inputsFrom = [ zksync zkstack ];

    packages = with pkgs; [
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
  });
in
devShell.overrideAttrs
  (old: { inherit (newshell) buildInputs nativeBuildInputs; })

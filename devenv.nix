{ pkgs, lib, config, inputs, ... }:

{
  cachix.enable = false;

  packages = [ pkgs.git pkgs.rustup ]
             ++ lib.optionals pkgs.stdenv.targetPlatform.isDarwin [
               pkgs.libiconv
               pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
             ];

  env = {
    ZKSYNC_USE_CUDA_STUBS = "true";
  };

  # services.postgres.enable = true;

  # https://devenv.sh/scripts/
  scripts.hello.exec = ''
    echo hello from $GREET
  '';

  enterShell = ''
    hello
    git --version
  '';

  # https://devenv.sh/tests/
  enterTest = ''
  '';

  # https://devenv.sh/pre-commit-hooks/
  # pre-commit.hooks.shellcheck.enable = true;
}

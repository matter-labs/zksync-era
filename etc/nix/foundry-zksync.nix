{ pkgs
, system
, lib
, stdenv
, ...
}:
let
  version = "v0.0.15";

  # use `nix-prefetch-url <URL>` to update the hashes
  # or the script `./update-foundry-zksync.sh v0.0.15`
  linux_amd_bin_url = "https://github.com/matter-labs/foundry-zksync/releases/download/foundry-zksync-${version}/foundry_zksync_${version}_linux_amd64.tar.gz";
  linux_amd_bin_sha = "0ra6yab8jj9i002zxw2g49q4s17xcbmx417kb8h0gsxi10yn9i3r";

  linux_arm_bin_url = "https://github.com/matter-labs/foundry-zksync/releases/download/foundry-zksync-${version}/foundry_zksync_${version}_linux_arm64.tar.gz";
  linux_arm_bin_sha = "1x2v6gk0qv92wn8xd6f45j8f233hah1psks4pbcc14l97c9w9y6k";

  darwin_amd_bin_url = "https://github.com/matter-labs/foundry-zksync/releases/download/foundry-zksync-${version}/foundry_zksync_${version}_darwin_amd64.tar.gz";
  darwin_amd_bin_sha = "0fnxn5jc6y5fl76q3jlpligh1876l0q6l5vcfpivjbdfvp03wcsp";

  darwin_arm_bin_url = "https://github.com/matter-labs/foundry-zksync/releases/download/foundry-zksync-${version}/foundry_zksync_${version}_darwin_arm64.tar.gz";
  darwin_arm_bin_sha = "1yljrms7llwdbqwfsqmmr7brvkqnn43byxzb5sv38vaa8c65cpzv";

  # Map of system identifiers to their specific source URL and SHA256 hash
  sourcesBySystem = {
    "x86_64-linux" = { url = linux_amd_bin_url; sha256 = linux_amd_bin_sha; };
    "aarch64-linux" = { url = linux_arm_bin_url; sha256 = linux_arm_bin_sha; };
    "x86_64-darwin" = { url = darwin_amd_bin_url; sha256 = darwin_amd_bin_sha; };
    "aarch64-darwin" = { url = darwin_arm_bin_url; sha256 = darwin_arm_bin_sha; };
  };

  # Get the attributes for the current target system
  # The `system` argument (e.g., "x86_64-linux") is used directly as the key.
  # If the system is not in our map, throw an error.
  targetSystemAttrs = sourcesBySystem.${system} or
    (lib.throw "Unsupported system: ${system}. Supported systems are: ${lib.concatStringsSep ", " (lib.attrNames sourcesBySystem)}");
in
pkgs.stdenv.mkDerivation {
  name = "foundry-zksync-${version}";
  src = pkgs.fetchurl {
    # Use the URL and SHA256 from the selected system attributes
    url = targetSystemAttrs.url;
    sha256 = targetSystemAttrs.sha256;
  };
  phases = [ "installPhase" "patchPhase" ];
  installPhase = ''
    mkdir -p $out/bin
    cd $out/bin
    tar xvzf $src
    chmod +x *
  '';

  prePatch = lib.optionalString stdenv.isDarwin ''
    set -x
    install_name_tool -change /opt/homebrew/opt/libusb/lib/libusb-1.0.0.dylib "${pkgs.libusb1}/lib/libusb-1.0.0.dylib" "$out/bin/forge"
    install_name_tool -change /opt/homebrew/opt/libusb/lib/libusb-1.0.0.dylib "${pkgs.libusb1}/lib/libusb-1.0.0.dylib" "$out/bin/cast"
  '';
}

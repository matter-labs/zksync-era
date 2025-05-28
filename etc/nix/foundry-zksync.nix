{ pkgs
, system
, lib
, stdenv
, ...
}:
let
  version = "v0.0.14";

  # use `nix-prefetch-url <URL>` to update the hashes

  linux_amd_bin_url = "https://github.com/matter-labs/foundry-zksync/releases/download/foundry-zksync-${version}/foundry_zksync_${version}_linux_amd64.tar.gz";
  linux_amd_bin_sha = "108da9djxws30z7apr2p2dzzpr1ngi4nqx90lgmgrpsj5sf2pqmz";

  linux_arm_bin_url = "https://github.com/matter-labs/foundry-zksync/releases/download/foundry-zksync-${version}/foundry_zksync_${version}_linux_arm64.tar.gz";
  linux_arm_bin_sha = "1zdx72crr6mx835hzgmcm883qdf5j806nbcx6mh5afggxq3v645z";

  darwin_amd_bin_url = "https://github.com/matter-labs/foundry-zksync/releases/download/foundry-zksync-${version}/foundry_zksync_${version}_darwin_amd64.tar.gz";
  darwin_amd_bin_sha = "07aw374hf31ix8vlyxkaw8iyq8i1dyi9hrs6wnrg3f31hp78bbwg";

  darwin_arm_bin_url = "https://github.com/matter-labs/foundry-zksync/releases/download/foundry-zksync-${version}/foundry_zksync_${version}_darwin_arm64.tar.gz";
  darwin_arm_bin_sha = "0x7w7ladfscnd2xpqj4l26606f3gy869nr8yrp0vvmbkh8awmi9b";

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

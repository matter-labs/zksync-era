# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2024-2025 Matter Labs
{ buildEnv
, curl
, dockerTools
, nixsgx
, openssl
, strace
, airbender_prover
, teepot
, ...
}:
dockerTools.buildLayeredImage {
  name = "zksync-airbender-prover-tdx";

  config = {
    Entrypoint = [ "/bin/sh" "-c" ];
    Cmd = [
      ''
        ${teepot.teepot.tee_key_preexec}/bin/tee-key-preexec \
          --env-prefix TEE_PROVER_ -- \
          ${airbender_prover}/bin/zksync_airbender_prover
      ''
    ];
    Env = [
      "SSL_CERT_FILE=/etc/ssl/certs/ca-bundle.crt"
    ];
  };

  contents = buildEnv {
    name = "image-root";

    paths = with dockerTools;[
      strace
      openssl.out
      curl.out
      nixsgx.sgx-dcap.quote_verify
      nixsgx.sgx-dcap.default_qpl
      usrBinEnv
      binSh
      caCertificates
      fakeNss
    ];
    pathsToLink = [ "/bin" "/lib" "/etc" "/share" "/tmp" ];
  };
}

{ lib
, pkgs
, teepot
, tee_prover
, container-name ? "zksync-tee-prover-azure"
, isAzure ? true
, tag ? null
, ...
}:
let
  name = container-name;
  entrypoint = "${teepot.teepot.tee_key_preexec}/bin/tee-key-preexec";
in
pkgs.lib.tee.sgxGramineContainer
  {
    inherit name;
    inherit tag;

    packages = [ teepot.teepot.tee_key_preexec tee_prover ];
    inherit entrypoint;
    inherit isAzure;

    manifest = {
      loader = {
        argv = [
          entrypoint
          "--env-prefix"
          "TEE_PROVER_"
          "--"
          "${tee_prover}/bin/zksync_tee_prover"
        ];

        log_level = "error";

        env = {
          TEE_PROVER_API_URL.passthrough = true;
          TEE_PROVER_MAX_RETRIES.passthrough = true;
          TEE_PROVER_INITIAL_RETRY_BACKOFF_SEC.passthrough = true;
          TEE_PROVER_RETRY_BACKOFF_MULTIPLIER.passthrough = true;
          TEE_PROVER_MAX_BACKOFF_SEC.passthrough = true;
          API_PROMETHEUS_LISTENER_PORT.passthrough = true;
          API_PROMETHEUS_PUSHGATEWAY_URL.passthrough = true;
          API_PROMETHEUS_PUSH_INTERVAL_MS.passthrough = true;

          ### DEBUG ###
          RUST_BACKTRACE = "1";
          RUST_LOG = "warning,zksync_tee_prover=debug";
        };
      };

      sgx = {
        edmm_enable = false;
        enclave_size = "8G";
        max_threads = 128;
      };
    };
  } // {
  meta = {
    description = "SGX on Azure container for the ZKsync TEE prover";
    homepage = "https://github.com/matter-labs/zksync-era/tree/main/core/bin/zksync_tee_prover";
    platforms = [ "x86_64-linux" ];
    license = [ lib.licenses.asl20 lib.licenses.mit ];
  };
}

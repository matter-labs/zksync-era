{ lib
, container-tee-prover-azure
, ...
}: container-tee-prover-azure.overrideAttrs
  {
    isAzure = false;
    container-name = "zksync-tee-prover-dcap";
  }
  // {
  meta = {
    description = "SGX DCAP container for the ZKsync TEE prover";
    homepage = "https://github.com/matter-labs/zksync-era/tree/main/core/bin/zksync_tee_prover";
    platforms = [ "x86_64-linux" ];
    license = [ lib.licenses.asl20 lib.licenses.mit ];
  };
}


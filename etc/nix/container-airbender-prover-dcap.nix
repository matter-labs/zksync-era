{ lib
, container-airbender-prover-azure
, ...
}: container-airbender-prover-azure.overrideAttrs
  {
    isAzure = false;
    container-name = "zksync-airbender-prover-dcap";
  }
  // {
  meta = {
    description = "SGX DCAP container for the ZKsync Airbender prover";
    homepage = "https://github.com/matter-labs/zksync-era/tree/main/core/bin/zksync_airbender_prover";
    platforms = [ "x86_64-linux" ];
    license = [ lib.licenses.asl20 lib.licenses.mit ];
  };
}


import "@matterlabs/hardhat-zksync-solc";
import "@nomiclabs/hardhat-vyper";
import "@matterlabs/hardhat-zksync-vyper";

export default {
  zksolc: {
    version: "1.5.10",
    compilerSource: "binary",
    settings: {
      enableEraVMExtensions: true,
    },
  },
  zkvyper: {
    version: "1.5.4",
    compilerSource: "binary",
  },
  networks: {
    hardhat: {
      zksync: false,
    },
  },
  solidity: {
    version: "0.8.26",
    eraVersion: "1.0.1",
    settings: {
      evmVersion: "cancun",
    },
  },
  vyper: {
    version: "0.3.10",
  },
};

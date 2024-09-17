import '@matterlabs/hardhat-zksync-solc';
import '@nomiclabs/hardhat-vyper';
import '@matterlabs/hardhat-zksync-vyper';

export default {
    zksolc: {
        version: '1.5.3',
        compilerSource: 'binary',
        settings: {
            enableEraVMExtensions: true
        }
    },
    zkvyper: {
        version: '1.5.4',
        compilerSource: 'binary'
    },
    networks: {
        hardhat: {
            zksync: true
        }
    },
    solidity: {
        version: '0.8.18',
        eraVersion: '1.0.1'
    },
    vyper: {
        version: '0.3.10'
    }
};

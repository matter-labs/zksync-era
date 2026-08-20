import '@matterlabs/hardhat-zksync-solc';
import '@nomiclabs/hardhat-vyper';
import '@matterlabs/hardhat-zksync-vyper';

export default {
    zksolc: {
        // Highest release accepted by the pinned Hardhat plugin used by this legacy test suite.
        version: '1.5.15',
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
        version: '0.8.26',
        eraVersion: '1.0.2',
        settings: {
            evmVersion: 'cancun'
        }
    },
    vyper: {
        version: '0.3.10'
    }
};

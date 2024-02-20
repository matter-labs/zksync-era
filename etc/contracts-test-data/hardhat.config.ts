import '@matterlabs/hardhat-zksync-solc';

export const COMPILER_PATH = `${process.env.ZKSYNC_HOME}/local-compiler/zksolc`;

export default {
    zksolc: {
        // version: 'prerelease-0640c18-test-zkvm-v1.5.0',
        compilerSource: 'binary',
        settings: {
            compilerPath: COMPILER_PATH,
            isSystem: true
        }
    },
    networks: {
        hardhat: {
            zksync: true
        }
    },
    solidity: {
        version: '0.8.24',
        settings: {
            evmVersion: 'cancun'
        }
    }
};

import '@matterlabs/hardhat-zksync-solc';

export default {
    zksolc: {
        // version: 'prerelease-0640c18-test-zkvm-v1.5.0',
        compilerSource: 'binary',
        settings: {
            compilerPath:
                'https://github.com/matter-labs/era-compiler-solidity/releases/download/prerelease-a167aa3-code4rena/zksolc-macosx-arm64-v1.5.0',
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

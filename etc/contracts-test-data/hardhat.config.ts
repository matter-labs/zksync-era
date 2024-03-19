import '@matterlabs/hardhat-zksync-solc';

export default {
    zksolc: {
        compilerSource: 'binary',
        settings: {
            compilerPath:
                'https://github.com/matter-labs/era-compiler-solidity/releases/download/prerelease-e880101-test-features/zksolc-macosx-arm64',
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

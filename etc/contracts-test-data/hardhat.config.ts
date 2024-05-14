import '@matterlabs/hardhat-zksync-solc';

const COMPILER_VERSION = '1.5.0';
const PRE_RELEASE_VERSION = 'prerelease-a167aa3-code4rena';
function getZksolcUrl(): string {
    // @ts-ignore
    const platform = { darwin: 'macosx', linux: 'linux', win32: 'windows' }[process.platform];
    // @ts-ignore
    const toolchain = { linux: '-musl', win32: '-gnu', darwin: '' }[process.platform];
    const arch = process.arch === 'x64' ? 'amd64' : process.arch;
    const ext = process.platform === 'win32' ? '.exe' : '';

    return `https://github.com/matter-labs/era-compiler-solidity/releases/download/${PRE_RELEASE_VERSION}/zksolc-${platform}-${arch}${toolchain}-v${COMPILER_VERSION}${ext}`;
}

export default {
    zksolc: {
        compilerSource: 'binary',
        settings: {
            compilerPath: getZksolcUrl(),
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

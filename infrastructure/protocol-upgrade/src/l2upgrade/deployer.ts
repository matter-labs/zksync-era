import { spawn } from 'utils';

export async function callSystemContractDeployer(
    l1RpcProvider: string,
    privateKey: string,
    l2RpcProvider: string,
    gasPrice: string,
    nonce: string,
    bootloader: boolean,
    defaultAA: boolean,
    evmSimulator: boolean,
    systemContracts: boolean,
    file: string
) {
    const cwd = process.cwd();
    process.chdir(`${process.env.ZKSYNC_HOME}/contracts/system-contracts`);
    let argsString = '';
    if (bootloader) {
        argsString += ' --bootloader';
    }
    if (defaultAA) {
        argsString += ' --default-aa';
    }
    if (evmSimulator) {
        argsString += '--evm-simulator';
    }
    if (systemContracts) {
        argsString += ' --system-contracts';
    }
    if (file) {
        argsString += ` --file ${file}`;
    }
    if (gasPrice) {
        argsString += ` --gas-price ${gasPrice}`;
    }
    if (nonce) {
        argsString += ` --nonce ${nonce}`;
    }
    if (l1RpcProvider) {
        argsString += ` --l1Rpc ${l1RpcProvider}`;
    }
    if (l2RpcProvider) {
        argsString += ` --l2Rpc ${l2RpcProvider}`;
    }
    if (privateKey) {
        argsString += ` --private-key ${privateKey}`;
    }
    await spawn(`yarn deploy-preimages ${argsString}`);
    process.chdir(cwd);
}

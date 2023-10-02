import { spawn } from 'zk/build/utils';

export async function callFacetDeployer(
    l1RpcProvider: string,
    privateKey: string,
    gasPrice: string,
    create2Address: string,
    nonce: string,
    executor: boolean,
    governance: boolean,
    diamondCut: boolean,
    getters: boolean,
    mailbox: boolean,
    file: string
) {
    const cwd = process.cwd();
    process.chdir(`${process.env.ZKSYNC_HOME}/contracts/ethereum/`);
    let argsString = '';
    if (executor) {
        argsString += ' --executor';
    }
    if (governance) {
        argsString += ' --governance';
    }
    if (diamondCut) {
        argsString += ' --diamondCut';
    }
    if (getters) {
        argsString += ' --getters';
    }
    if (mailbox) {
        argsString += ' --mailbox';
    }
    if (file) {
        argsString += ` --file ${file}`;
    }
    if (gasPrice) {
        argsString += ` --gasPrice ${gasPrice}`;
    }
    if (nonce) {
        argsString += ` --nonce ${nonce}`;
    }
    if (l1RpcProvider) {
        argsString += ` --l1Rpc ${l1RpcProvider}`;
    }
    if (privateKey) {
        argsString += ` --privateKey ${privateKey}`;
    }
    if (create2Address) {
        argsString += ` --create2-address ${create2Address}`;
    }
    await spawn(`yarn upgrade-system facets deploy ${argsString}`);
    process.chdir(cwd);
}

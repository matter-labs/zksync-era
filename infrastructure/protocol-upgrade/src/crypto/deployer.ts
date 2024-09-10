import { spawn } from 'utils';

export async function deployVerifier(
    l1Rpc: string,
    privateKey: string,
    create2Address: string,
    file: string,
    nonce?: number,
    gasPrice?: number,
    useTestnetVerifier?: boolean
) {
    const cwd = process.cwd();
    process.chdir(`${process.env.ZKSYNC_HOME}/contracts/l1-contracts/`);
    let argsString = '';
    if (l1Rpc) {
        argsString += ` --l1rpc ${l1Rpc}`;
    }
    if (privateKey) {
        argsString += ` --private-key ${privateKey}`;
    }
    if (nonce) {
        argsString += ` --nonce ${nonce}`;
    }
    if (gasPrice) {
        argsString += ` --gas-price ${gasPrice}`;
    }
    if (useTestnetVerifier) {
        argsString += ` --use-testnet-verifier ${useTestnetVerifier}`;
    }

    create2Address = create2Address ?? process.env.CONTRACTS_CREATE2_FACTORY_ADDR;
    argsString += ` --create2-address ${create2Address}`;

    argsString += ` --file ${file}`;

    await spawn(`yarn upgrade-system verifier deploy ${argsString}`);

    process.chdir(cwd);
}

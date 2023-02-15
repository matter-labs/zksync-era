import * as zkweb3 from 'zksync-web3';
import * as ethers from 'ethers';
import * as path from 'path';
import * as fs from 'fs';
import * as axios from 'axios';

async function depositTestAccounts() {
    const ethProvider = new ethers.providers.JsonRpcProvider(
        process.env.L1_RPC_ADDRESS || process.env.ETH_CLIENT_WEB3_URL
    );
    const testConfigPath = path.join(process.env.ZKSYNC_HOME!, `etc/test_config/constant`);
    const ethTestConfig = JSON.parse(fs.readFileSync(`${testConfigPath}/eth.json`, { encoding: 'utf-8' }));
    const ethWallet = ethers.Wallet.fromMnemonic(ethTestConfig.test_mnemonic as string, "m/44'/60'/0'/0/0").connect(
        ethProvider
    );
    const web3Provider = new zkweb3.Provider(process.env.ZKSYNC_WEB3_API_URL || 'http://localhost:3050');
    const syncWallet = new zkweb3.Wallet(ethWallet.privateKey, web3Provider, ethProvider);

    const testAccountPks = process.env.API_WEB3_JSON_RPC_ACCOUNT_PKS!.split(',');
    let handles = [];
    for (const key of testAccountPks) {
        const wallet = new zkweb3.Wallet(key, web3Provider, ethProvider);
        handles.push(
            await syncWallet.deposit({
                token: ethers.constants.AddressZero,
                to: wallet.address,
                amount: ethers.utils.parseEther('10000')
            })
        );
    }
    for (const handle of handles) {
        await handle.wait();
    }
}

async function sendBytecodeFromFolder(folderPath: string) {
    const files = fs.readdirSync(folderPath);
    for (const file of files) {
        const filePath = path.join(folderPath, file);
        if (fs.lstatSync(filePath).isDirectory()) {
            await sendBytecodeFromFolder(filePath);
        } else {
            if (filePath.includes('.json')) {
                const text = fs.readFileSync(filePath, 'utf-8');
                const data = JSON.parse(text);
                if ('bytecode' in data) {
                    const req = {
                        jsonrpc: '2.0',
                        method: 'zks_setKnownBytecode',
                        params: [data.bytecode],
                        id: 1
                    };
                    const resp = await axios.default.post('http://127.0.0.1:3050', req);
                    console.log(filePath + ': ' + resp.data.toString());
                }
            }
        }
    }
}

async function main() {
    await depositTestAccounts();
    await sendBytecodeFromFolder(`${process.env.ZKSYNC_HOME}/etc/openzeppelin-contracts/artifacts-zk`);
}

main()
    .then(() => {
        console.log('Finished successfully');
    })
    .catch((err) => {
        console.log('err: ' + err);
    });

import { ethers } from 'ethers';
import * as fs from 'fs';
import * as path from 'path';

interface WalletKey {
    address: string;
    privateKey: string;
}

export function getWalletKeys(): WalletKey[] {
    const testConfigPath = path.join(process.env.ZKSYNC_HOME as string, `etc/test_config/constant`);
    const ethTestConfig = JSON.parse(fs.readFileSync(`${testConfigPath}/eth.json`, { encoding: 'utf-8' }));
    const NUM_TEST_WALLETS = 10;
    const baseWalletPath = "m/44'/60'/0'/0/";
    const walletKeys: WalletKey[] = [];
    for (let i = 0; i < NUM_TEST_WALLETS; ++i) {
        const ethWallet = ethers.Wallet.fromMnemonic(ethTestConfig.test_mnemonic as string, baseWalletPath + i);
        walletKeys.push({
            address: ethWallet.address,
            privateKey: ethWallet.privateKey
        });
    }
    return walletKeys;
}

export function getEthersProvider(): ethers.providers.JsonRpcProvider {
    return new ethers.providers.JsonRpcProvider(process.env.ETH_CLIENT_WEB3_URL || 'http://localhost:8545');
}

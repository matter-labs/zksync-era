import * as ethers from 'ethers';
import * as zksync from 'zksync-ethers';
import * as fs from 'fs';
import * as path from 'path';

export class Tester {
    public runningFee: Map<zksync.types.Address, bigint>;
    constructor(
        public ethProvider: ethers.Provider,
        public ethWallet: ethers.Wallet,
        public syncWallet: zksync.Wallet,
        public web3Provider: zksync.Provider
    ) {
        this.runningFee = new Map();
    }

    // prettier-ignore
    static async init(ethProviderAddress: string, web3JsonRpc: string) {
        const ethProvider = new ethers.JsonRpcProvider(ethProviderAddress);

        let ethWallet;
        if (process.env.MASTER_WALLET_PK) {
            ethWallet = new ethers.Wallet(process.env.MASTER_WALLET_PK);
        }
        else {
            ethProvider.pollingInterval = 100;

            const testConfigPath = path.join(process.env.ZKSYNC_HOME!, `etc/test_config/constant`);
            const ethTestConfig = JSON.parse(fs.readFileSync(`${testConfigPath}/eth.json`, { encoding: 'utf-8' }));
            const ethWalletHD = ethers.HDNodeWallet.fromMnemonic(
                ethers.Mnemonic.fromPhrase(ethTestConfig.test_mnemonic),
                "m/44'/60'/0'/0/0"
            );
            ethWallet = new ethers.Wallet(ethWalletHD.privateKey, ethProvider);
        }

        ethWallet = ethWallet.connect(ethProvider);
        const web3Provider = new zksync.Provider(web3JsonRpc);
        web3Provider.pollingInterval = 100; // It's OK to keep it low even on stage.
        const syncWallet = new zksync.Wallet(ethWallet.privateKey, web3Provider, ethProvider);


        // Since some tx may be pending on stage, we don't want to get stuck because of it.
        // In order to not get stuck transactions, we manually cancel all the pending txs.
        const latestNonce = await ethWallet.getNonce('latest');
        const pendingNonce = await ethWallet.getNonce('pending');
        const cancellationTxs = [];
        for (let nonce = latestNonce; nonce != pendingNonce; nonce++) {
            // For each transaction to override it, we need to provide greater fee. 
            // We would manually provide a value high enough (for a testnet) to be both valid
            // and higher than the previous one. It's OK as we'll only be charged for the bass fee
            // anyways. We will also set the miner's tip to 5 gwei, which is also much higher than the normal one.
            const maxFeePerGas = ethers.parseEther("0.00000025"); // 250 gwei
            const maxPriorityFeePerGas = ethers.parseEther("0.000000005"); // 5 gwei
            cancellationTxs.push(ethWallet.sendTransaction({ to: ethWallet.address, nonce, maxFeePerGas, maxPriorityFeePerGas }).then((tx) => tx.wait()));
        }
        if (cancellationTxs.length > 0) {
            await Promise.all(cancellationTxs);
            console.log(`Canceled ${cancellationTxs.length} pending transactions`);
        }

        return new Tester(ethProvider, ethWallet, syncWallet, web3Provider);
    }

    emptyWallet() {
        const walletHD = zksync.Wallet.createRandom();
        return new zksync.Wallet(walletHD.privateKey, this.web3Provider, this.ethProvider);
    }
}

import * as ethers from 'ethers';
import * as zkweb3 from 'zksync-web3';
import * as fs from 'fs';
import * as path from 'path';

type Network = string;

export class Tester {
    public runningFee: Map<zkweb3.types.Address, ethers.BigNumber>;
    constructor(
        public network: Network,
        public ethProvider: ethers.providers.Provider,
        public ethWallet: ethers.Wallet,
        public syncWallet: zkweb3.Wallet,
        public web3Provider: zkweb3.Provider
    ) {
        this.runningFee = new Map();
    }

    // prettier-ignore
    static async init(network: Network) {
        const ethProvider = new ethers.providers.JsonRpcProvider(process.env.L1_RPC_ADDRESS || process.env.ETH_CLIENT_WEB3_URL);

        let ethWallet;
        if (network == 'localhost') {
            ethProvider.pollingInterval = 100;

            const testConfigPath = path.join(process.env.ZKSYNC_HOME!, `etc/test_config/constant`);
            const ethTestConfig = JSON.parse(fs.readFileSync(`${testConfigPath}/eth.json`, { encoding: 'utf-8' }));
            ethWallet = ethers.Wallet.fromMnemonic(
                ethTestConfig.test_mnemonic as string,
                "m/44'/60'/0'/0/0"
            )
        }
        else {
            ethWallet = new ethers.Wallet(process.env.MASTER_WALLET_PK!);
        }
        ethWallet = ethWallet.connect(ethProvider);
        const web3Provider = new zkweb3.Provider(process.env.ZKSYNC_WEB3_API_URL || "http://localhost:3050");
        web3Provider.pollingInterval = 100; // It's OK to keep it low even on stage.
        const syncWallet = new zkweb3.Wallet(ethWallet.privateKey, web3Provider, ethProvider);


        // Since some tx may be pending on stage, we don't want to get stuck because of it.
        // In order to not get stuck transactions, we manually cancel all the pending txs.
        const latestNonce = await ethWallet.getTransactionCount('latest');
        const pendingNonce = await ethWallet.getTransactionCount('pending');
        const cancellationTxs = [];
        for (let nonce = latestNonce; nonce != pendingNonce; nonce++) {
            // For each transaction to override it, we need to provide greater fee. 
            // We would manually provide a value high enough (for a testnet) to be both valid
            // and higher than the previous one. It's OK as we'll only be charged for the bass fee
            // anyways. We will also set the miner's tip to 5 gwei, which is also much higher than the normal one.
            const maxFeePerGas = ethers.utils.parseEther("0.00000025"); // 250 gwei
            const maxPriorityFeePerGas = ethers.utils.parseEther("0.000000005"); // 5 gwei
            cancellationTxs.push(ethWallet.sendTransaction({ to: ethWallet.address, nonce, maxFeePerGas, maxPriorityFeePerGas }).then((tx) => tx.wait()));
        }
        if (cancellationTxs.length > 0) {
            await Promise.all(cancellationTxs);
            console.log(`Canceled ${cancellationTxs.length} pending transactions`);
        }

        return new Tester(network, ethProvider, ethWallet, syncWallet, web3Provider);
    }

    emptyWallet() {
        return zkweb3.Wallet.createRandom().connect(this.web3Provider).connectToL1(this.ethProvider);
    }
}

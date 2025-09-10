import * as path from 'path';
import * as ethers from 'ethers';
import * as zksync from 'zksync-ethers';
import { getMainWalletPk } from 'highlevel-test-tools/src/wallets';
import { L1Token, getToken } from 'utils/src/tokens';

export class Tester {
    public runningFee: Map<zksync.types.Address, bigint>;
    constructor(
        public ethProvider: ethers.Provider,
        public ethWallet: ethers.Wallet,
        public syncWallet: zksync.Wallet,
        public web3Provider: zksync.Provider,
        public token: L1Token
    ) {
        this.runningFee = new Map();
    }

    // prettier-ignore
    static async init(ethProviderAddress: string, web3JsonRpc: string) {
        const pathToHome = path.join(__dirname, '../../../..');

        const ethProvider = new ethers.JsonRpcProvider(ethProviderAddress);

        const chainName = process.env.CHAIN_NAME!!;
        let ethWallet = new ethers.Wallet(getMainWalletPk(chainName));

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

        const baseTokenAddress = await web3Provider.getBaseTokenContractAddress();

        const { token, } = getToken(pathToHome, baseTokenAddress);

        return new Tester(ethProvider, ethWallet, syncWallet, web3Provider, token);
    }

    emptyWallet() {
        const walletHD = zksync.Wallet.createRandom();
        return new zksync.Wallet(walletHD.privateKey, this.web3Provider, this.ethProvider);
    }
}

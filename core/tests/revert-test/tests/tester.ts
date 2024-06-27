import { expect } from 'chai';
import * as ethers from 'ethers';
import * as zksync from 'zksync-ethers';
import * as fs from 'fs';
import * as path from 'path';

const BASE_ERC20_TO_MINT = ethers.parseEther('100');

export class Tester {
    public runningFee: Map<zksync.types.Address, bigint>;
    constructor(
        public ethProvider: ethers.Provider,
        public ethWallet: ethers.Wallet,
        public syncWallet: zksync.Wallet,
        public web3Provider: zksync.Provider,
        public hyperchainAdmin: ethers.Wallet, // We need to add validator to ValidatorTimelock with admin rights
        public isETHBasedChain: boolean,
        public baseTokenAddress: string
    ) {
        this.runningFee = new Map();
    }

    // prettier-ignore
    static async init(l1_rpc_addr: string, l2_rpc_addr: string) : Promise<Tester> {
        const ethProvider = new ethers.JsonRpcProvider(l1_rpc_addr);
        ethProvider.pollingInterval = 100;

        const testConfigPath = path.join(process.env.ZKSYNC_HOME!, `etc/test_config/constant`);
        const ethTestConfig = JSON.parse(fs.readFileSync(`${testConfigPath}/eth.json`, { encoding: 'utf-8' }));
        const ethWalletHD = ethers.HDNodeWallet.fromMnemonic(
            ethers.Mnemonic.fromPhrase(ethTestConfig.test_mnemonic),
            "m/44'/60'/0'/0/0"
        );
        const ethWallet = new ethers.Wallet(ethWalletHD.privateKey, ethProvider);
        const hyperchainAdminHD = ethers.HDNodeWallet.fromMnemonic(
            ethers.Mnemonic.fromPhrase(ethTestConfig.mnemonic),
            "m/44'/60'/0'/0/1"
        );
        const hyperchainAdmin = new ethers.Wallet(hyperchainAdminHD.privateKey, ethProvider);
        const web3Provider = new zksync.Provider(l2_rpc_addr);
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

        const baseTokenAddress = process.env.CONTRACTS_BASE_TOKEN_ADDR!;
        const isETHBasedChain = baseTokenAddress == zksync.utils.ETH_ADDRESS_IN_CONTRACTS;

        return new Tester(ethProvider, ethWallet, syncWallet, web3Provider, hyperchainAdmin, isETHBasedChain, baseTokenAddress);
    }

    /// Ensures that the main wallet has enough base token.
    /// This can not be done inside the `init` function because `init` function can be called before the
    /// L2 RPC is active, but we need the L2 RPC to get the base token address.
    async fundSyncWallet() {
        const baseTokenAddress = await this.syncWallet.provider.getBaseTokenContractAddress();
        if (!(baseTokenAddress === zksync.utils.ETH_ADDRESS_IN_CONTRACTS)) {
            const l1Erc20ABI = ['function mint(address to, uint256 amount)'];
            const l1Erc20Contract = new ethers.Contract(baseTokenAddress, l1Erc20ABI, this.ethWallet);
            await (await l1Erc20Contract.mint(this.ethWallet.address, BASE_ERC20_TO_MINT)).wait();
        }
    }

    async fundedWallet(ethAmount: bigint, l1Token: zksync.types.Address, tokenAmount: bigint) {
        const newWalletHD = zksync.Wallet.createRandom();
        const newWallet = new zksync.Wallet(newWalletHD.privateKey, this.web3Provider, this.ethProvider);

        let ethBalance = await this.syncWallet.getBalanceL1();
        expect(ethBalance > ethAmount, 'Insufficient eth balance to create funded wallet').to.be.true;

        // To make the wallet capable of requesting priority operations,
        // send ETH to L1.

        const tx1 = await this.syncWallet.ethWallet().sendTransaction({
            to: newWallet.address,
            value: ethAmount
        });
        await tx1.wait();

        // Funds the wallet with L1 token.

        let tokenBalance = await this.syncWallet.getBalanceL1(l1Token);
        expect(tokenBalance > tokenAmount, 'Insufficient token balance to create funded wallet').to.be.true;

        const erc20ABI = ['function transfer(address to, uint256 amount)'];
        const erc20Contract = new ethers.Contract(l1Token, erc20ABI, this.ethWallet);

        const tx2 = await erc20Contract.transfer(newWallet.address, tokenAmount);
        await tx2.wait();

        return newWallet;
    }

    emptyWallet() {
        const walletHD = zksync.Wallet.createRandom();
        return new zksync.Wallet(walletHD.privateKey, this.web3Provider, this.ethProvider);
    }
}

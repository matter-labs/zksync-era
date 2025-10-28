import * as ethers from 'ethers';
import * as zksync from 'zksync-ethers';
import { getMainWalletPk } from 'highlevel-test-tools/src/wallets';
import {createChainAndStartServer, TestChain, ChainType} from 'highlevel-test-tools/src/create-chain';
import path from 'path';
import { loadConfig } from 'utils/build/file-configs';

const RICH_WALLET_L1_BALANCE = ethers.parseEther('10.0');
const RICH_WALLET_L2_BALANCE = RICH_WALLET_L1_BALANCE;
const TEST_SUIT_NAME = 'AT_CHAIN_MIGRATION_TEST';
const pathToHome = path.join(__dirname, '../../../..');

// TODO: For now, only works with ETH chains
async function generateChainRichWallet(chainName: string): Promise<zksync.Wallet> {
    const generalConfig = loadConfig({
        pathToHome,
        chain: chainName,
        config: 'general.yaml'
    });
    const contractsConfig = loadConfig({
        pathToHome,
        chain: chainName,
        config: 'contracts.yaml'
    });
    const secretsConfig = loadConfig({
        pathToHome,
        chain: chainName,
        config: 'secrets.yaml'
    });
    const ethProviderAddress = secretsConfig.l1.l1_rpc_url;
    const web3JsonRpc = generalConfig.api.web3_json_rpc.http_url;

    const richWallet = new zksync.Wallet(getMainWalletPk('gateway'), new zksync.Provider(web3JsonRpc), new ethers.JsonRpcProvider(ethProviderAddress));

    // We assume that Gateway has "ETH" as the base token.
    // We deposit funds to ensure that the wallet is rich
    await (await richWallet.deposit({
        token: zksync.utils.ETH_ADDRESS_IN_CONTRACTS,
        amount: ethers.parseEther('10.0')
    })).wait();

    return richWallet;
}

// async function zkstackSpawnNewChain() {

// }

export class ChainHandler {
    // public name: string;
    public l2RichWallet: zksync.Wallet;

    public inner: TestChain;

    constructor(inner: TestChain, l2RichWallet: zksync.Wallet) {
        this.inner = inner;
        this.l2RichWallet = l2RichWallet;
    }

    async stopServer() {
        await this.inner.mainNode.kill();
    }

    async migrateToGateway() {

    }

    async migrateFromGateway() {

    }

    static async createNewChain(
        chainType: ChainType,
    ): Promise<ChainHandler> {
        const testChain = await createChainAndStartServer(
            chainType,
            TEST_SUIT_NAME
        );
        
        const handler = new ChainHandler(testChain, await generateChainRichWallet(testChain.chainName));

        return handler;
    }
}

export class L2ERC20Handler {
    public address: string;
    public l2Wallet: zksync.Wallet;
    public contract: zksync.Contract;

    constructor(address: string, l2Wallet: zksync.Wallet) {
        this.address = address;
        this.l2Wallet = l2Wallet;
        this.contract = new zksync.Contract(
            address,
            // FIXME: use erc20 ABI
            [],
            l2Wallet
        );
    }

    async withdraw(): Promise<WithdrawalHandler> {
        throw new Error('');
    }

    async migrateBalanceL2ToGW(): Promise<MigrationHandler> {
        throw new Error('');
    }

    async migrateBalanceGWtoL1(gwRichWallet: zksync.Wallet): Promise<MigrationHandler> {
        throw new Error('');
    }

    static async deployToken(l2Wallet: zksync.Wallet) {
        // FIXME actually deploy it.
        return new L2ERC20Handler(ethers.ZeroAddress, l2Wallet);
    }
}

export class L1ERC20Handler {
    public address: string;
    public l1Wallet: ethers.Wallet;
    public contract: zksync.Contract;


    constructor(address: string, l1Wallet: ethers.Wallet) {
        this.address = address;
        this.l1Wallet = l1Wallet;
        this.contract = new ethers.Contract(
            address,
            // FIXME: use erc20 ABI
            [],
            l1Wallet
        );
    }
    
    // We always wait for the deposit to be finalized.
    async deposit(chain: ChainHandler) {
        // TODO: actually perform deposit + wait for it.
    }

    async atL2SameWallet(chainHandler: ChainHandler) {
        // FIXME: actually obtain it
        const addrL2 = ethers.ZeroAddress;

        return new L2ERC20Handler(addrL2, new zksync.Wallet(
            this.l1Wallet.privateKey,
            chainHandler.l2RichWallet.provider,
            this.l1Wallet.provider!
        ));
    }

    static async deployToken(l1Wallet: ethers.Wallet) {
        // FIXME actually deploy it.
        return new L1ERC20Handler(ethers.ZeroAddress, l1Wallet);
    } 
}

export class WithdrawalHandler {
    public txHash: string;

    constructor(txHash: string, provider: zksync.Provider) {
        this.txHash = txHash;
    }

    async finalizeWithdrawal(l1RichWallet: ethers.Wallet) {

    }
}

export class MigrationHandler {
    public txHash: string;

    constructor(txHash: string, provider: zksync.Provider) {
        this.txHash = txHash;
    }

    async finalizeMigration(l1RichWallet: ethers.Wallet) {

    }
}

async function newSpawnNewChainZKStack() {

}


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

        return new Tester(ethProvider, ethWallet, syncWallet, web3Provider);
    }

    emptyWallet() {
        const walletHD = zksync.Wallet.createRandom();
        return new zksync.Wallet(walletHD.privateKey, this.web3Provider, this.ethProvider);
    }
}

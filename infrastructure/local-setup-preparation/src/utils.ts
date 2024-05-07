import { utils } from 'zksync-ethers';
import { ethers } from 'ethers';
import * as fs from 'fs';
import * as path from 'path';
import { ValidatorTimelockFactory } from '../../../contracts/l1-contracts/typechain/ValidatorTimelockFactory';
import { StateTransitionManagerFactory } from '../../../contracts/l1-contracts/typechain/StateTransitionManagerFactory';

interface WalletKey {
    address: string;
    privateKey: string;
}

// Cache object for storing contract instances
const contractCache = {
    contractMain: null,
    stm: null,
    validatorTimelock: null
};

// Function to get contract instances
async function getContractInstances() {
    const ethProvider = getEthersProvider();

    if (!contractCache.contractMain) {
        contractCache.contractMain = new ethers.Contract(
            process.env.CONTRACTS_DIAMOND_PROXY_ADDR,
            utils.ZKSYNC_MAIN_ABI,
            ethProvider
        );
    }

    if (!contractCache.stm) {
        const stateTransitionManagerAddr = await contractCache.contractMain.getStateTransitionManager();
        contractCache.stm = StateTransitionManagerFactory.connect(stateTransitionManagerAddr, ethProvider);
    }

    if (!contractCache.validatorTimelock) {
        const validatorTimelockAddr = await contractCache.stm.validatorTimelock();
        contractCache.validatorTimelock = ValidatorTimelockFactory.connect(validatorTimelockAddr, ethProvider);
    }

    return {
        contractMain: contractCache.contractMain,
        stm: contractCache.stm,
        validatorTimelock: contractCache.validatorTimelock
    };
}

export async function isOperator(chainId: string, walletAddress: string): Promise<boolean> {
    try {
        const { validatorTimelock } = await getContractInstances();
        const isOperator = await validatorTimelock.validators(chainId, walletAddress);
        return isOperator;
    } catch (error) {
        console.error('Error checking if address is an operator:', error);
        throw error;
    }
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

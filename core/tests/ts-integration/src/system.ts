import { BigNumber, BytesLike } from 'ethers';
import { ethers } from 'ethers';
import { Provider, utils } from 'zksync-web3';

const L1_CONTRACTS_FOLDER = `${process.env.ZKSYNC_HOME}/contracts/ethereum/artifacts/cache/solpp-generated-contracts`;
const DIAMOND_UPGRADE_INIT_ABI = new ethers.utils.Interface(
    require(`${L1_CONTRACTS_FOLDER}/zksync/upgrade-initializers/DiamondUpgradeInit1.sol/DiamondUpgradeInit1.json`).abi
);
const DIAMOND_CUT_FACET_ABI = new ethers.utils.Interface(
    require(`${L1_CONTRACTS_FOLDER}/zksync/facets/DiamondCut.sol/DiamondCutFacet.json`).abi
);

export interface ForceDeployment {
    // The bytecode hash to put on an address
    bytecodeHash: BytesLike;
    // The address on which to deploy the bytecodehash to
    newAddress: string;
    // Whether to call the constructor
    callConstructor: boolean;
    // The value with which to initialize a contract
    value: BigNumber;
    // The constructor calldata
    input: BytesLike;
}

// A minimized copy of the `diamondCut` function used in L1 contracts
function diamondCut(facetCuts: any[], initAddress: string, initCalldata: string): any {
    return {
        facetCuts,
        initAddress,
        initCalldata
    };
}

/**
 * Uses a small upgrade to deploy a contract via a forcedDeploy
 *
 * @param ethProvider The L1 provider.
 * @param l2Provider The zkSync provider.
 * @param deployments Array of forced deployments to perform.
 * @param factoryDeps Factory deps that should be included with this transaction.
 * @returns The receipt of the L2 transaction corresponding to the forced deployment
 */
export async function deployOnAnyLocalAddress(
    ethProvider: ethers.providers.Provider,
    l2Provider: Provider,
    deployments: ForceDeployment[],
    factoryDeps: BytesLike[]
): Promise<ethers.providers.TransactionReceipt> {
    const diamondUpgradeInitAddress = process.env.CONTRACTS_DIAMOND_UPGRADE_INIT_ADDR;

    // The same mnemonic as in the etc/test_config/eth.json
    const govMnemonic = require('../../../../etc/test_config/constant/eth.json').mnemonic;

    if (!diamondUpgradeInitAddress) {
        throw new Error('DIAMOND_UPGRADE_INIT_ADDRESS not set');
    }

    const govWallet = ethers.Wallet.fromMnemonic(govMnemonic, "m/44'/60'/0'/0/1").connect(ethProvider);

    const zkSyncContract = await l2Provider.getMainContractAddress();

    const zkSync = new ethers.Contract(zkSyncContract, utils.ZKSYNC_MAIN_ABI, govWallet);

    // In case there is some pending upgrade there, we cancel it
    const upgradeProposalState = await zkSync.getUpgradeProposalState();
    if (upgradeProposalState != 0) {
        const currentProposalHash = await zkSync.getProposedUpgradeHash();
        await zkSync.connect(govWallet).cancelUpgradeProposal(currentProposalHash);
    }

    // Encode data for the upgrade call
    const encodedParams = utils.CONTRACT_DEPLOYER.encodeFunctionData('forceDeployOnAddresses', [deployments]);

    // Prepare the diamond cut data
    const upgradeInitData = DIAMOND_UPGRADE_INIT_ABI.encodeFunctionData('forceDeployL2Contract', [
        encodedParams,
        factoryDeps,
        parseInt(process.env.CONTRACTS_PRIORITY_TX_MAX_GAS_LIMIT as string)
    ]);

    const upgradeParam = diamondCut([], diamondUpgradeInitAddress, upgradeInitData);
    const currentProposalId = (await zkSync.getCurrentProposalId()).add(1);
    // Get transaction data of the `proposeTransparentUpgrade`
    const proposeTransparentUpgrade = DIAMOND_CUT_FACET_ABI.encodeFunctionData('proposeTransparentUpgrade', [
        upgradeParam,
        currentProposalId
    ]);

    // Get transaction data of the `executeUpgrade`
    const executeUpgrade = DIAMOND_CUT_FACET_ABI.encodeFunctionData('executeUpgrade', [
        upgradeParam,
        ethers.constants.HashZero
    ]);

    // Proposing the upgrade
    await (
        await govWallet.sendTransaction({
            to: zkSyncContract,
            data: proposeTransparentUpgrade,
            gasLimit: BigNumber.from(10000000)
        })
    ).wait();

    // Finalize the upgrade
    const receipt = await (
        await govWallet.sendTransaction({
            to: zkSyncContract,
            data: executeUpgrade,
            gasLimit: BigNumber.from(10000000)
        })
    ).wait();

    const txHash = utils.getL2HashFromPriorityOp(receipt, zkSyncContract);

    return await l2Provider.waitForTransaction(txHash);
}

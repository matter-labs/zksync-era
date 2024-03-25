import { BigNumber, BytesLike, ethers } from 'ethers';
import { Provider, utils } from 'zksync-ethers';

const L1_CONTRACTS_FOLDER = `${process.env.ZKSYNC_HOME}/contracts/l1-contracts/artifacts/cache/solpp-generated-contracts`;
const L1_CONTRACTS_FOLDER_2 = `${process.env.ZKSYNC_HOME}/contracts/l1-contracts/artifacts-forge`;
const DIAMOND_UPGRADE_INIT_ABI = new ethers.utils.Interface(
    require(`${L1_CONTRACTS_FOLDER_2}/zksync/upgrade-initializers/DiamondUpgradeInit1.sol/DiamondUpgradeInit1.json`).abi
);
const GOVERNANCE_ABI = new ethers.utils.Interface(
    require(`${L1_CONTRACTS_FOLDER}/governance/Governance.sol/Governance.json`).abi
);
const ADMIN_FACET_ABI = new ethers.utils.Interface(
    require(`${L1_CONTRACTS_FOLDER}/state-transition/chain-deps/facets/Admin.sol/AdminFacet.json`).abi
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

    const stateTransitionContract = await l2Provider.getMainContractAddress();

    const stateTransition = new ethers.Contract(stateTransitionContract, utils.ZKSYNC_MAIN_ABI, govWallet);
    const governanceContractAddr = await stateTransition.getAdmin();

    // Encode data for the upgrade call
    const encodedParams = utils.CONTRACT_DEPLOYER.encodeFunctionData('forceDeployOnAddresses', [deployments]);

    // Prepare the diamond cut data
    const upgradeInitData = DIAMOND_UPGRADE_INIT_ABI.encodeFunctionData('forceDeployL2Contract', [
        encodedParams,
        factoryDeps,
        parseInt(process.env.CONTRACTS_PRIORITY_TX_MAX_GAS_LIMIT as string)
    ]);

    const upgradeParam = diamondCut([], diamondUpgradeInitAddress, upgradeInitData);

    // Prepare calldata for upgrading diamond proxy
    const diamondProxyUpgradeCalldata = ADMIN_FACET_ABI.encodeFunctionData('executeUpgrade', [upgradeParam]);

    const call = {
        target: stateTransitionContract,
        value: 0,
        data: diamondProxyUpgradeCalldata
    };
    const governanceOperation = {
        calls: [call],
        predecessor: ethers.constants.HashZero,
        salt: ethers.constants.HashZero
    };

    // Get transaction data of the `scheduleTransparent`
    const scheduleTransparentOperation = GOVERNANCE_ABI.encodeFunctionData('scheduleTransparent', [
        governanceOperation,
        0 // delay
    ]);

    // Get transaction data of the `execute`
    const executeOperation = GOVERNANCE_ABI.encodeFunctionData('execute', [governanceOperation]);

    // Proposing the upgrade
    await (
        await govWallet.sendTransaction({
            to: governanceContractAddr,
            data: scheduleTransparentOperation,
            gasLimit: BigNumber.from(10000000)
        })
    ).wait();

    // Finalize the upgrade
    const receipt = await (
        await govWallet.sendTransaction({
            to: governanceContractAddr,
            data: executeOperation,
            gasLimit: BigNumber.from(10000000)
        })
    ).wait();

    const txHash = utils.getL2HashFromPriorityOp(receipt, stateTransitionContract);

    return await l2Provider.waitForTransaction(txHash);
}

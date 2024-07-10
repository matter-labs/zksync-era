/**
 * This suite contains tests checking default ERC-20 contract behavior.
 */

import { TestMaster } from '../src/index';
import { Token } from '../src/types';
// import { shouldChangeTokenBalances, shouldOnlyTakeFee } from '../src/modifiers/balance-checker';

import * as zksync from 'zksync-ethers';
// import { BigNumber, utils as etherUtils } from 'ethers';
import * as ethers from 'ethers';
// import { scaledGasPrice, waitUntilBlockFinalized } from '../src/helpers';
// import { L2_DEFAULT_ETH_PER_ACCOUNT } from '../src/context-owner';
import { IBridgehubFactory } from '../../../../contracts/l1-contracts/typechain/IBridgehubFactory';
import { IL1NativeTokenVaultFactory } from '../../../../contracts/l1-contracts/typechain/IL1NativeTokenVaultFactory';
import {
    L2_ASSET_ROUTER_ADDRESS,
    L2_BRIDGEHUB_ADDRESS,
    L2_NATIVE_TOKEN_VAULT_ADDRESS,
    REQUIRED_L2_GAS_PRICE_PER_PUBDATA
} from '../../../../contracts/l1-contracts/src.ts/utils';
import { RetryProvider } from '../src/retry-provider';
import { ETH_ADDRESS_IN_CONTRACTS } from 'zksync-ethers/build/utils';

describe('Interop checks', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let bob: zksync.Wallet;
    let bobOtherChain: zksync.Wallet;
    let tokenDetails: Token;
    // let baseTokenDetails: Token;
    let aliceErc20: zksync.Contract;
    // let l2NativeTokenVault: ethers.Contract;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
        bob = testMaster.newEmptyAccount();

        let l2Provider = new RetryProvider(
            {
                url: 'http://localhost:3050',
                timeout: 1200 * 1000
            },
            undefined,
            testMaster.reporter
        );
        bobOtherChain = new zksync.Wallet(bob.privateKey, l2Provider, bob.providerL1!);

        tokenDetails = testMaster.environment().erc20Token;
        // baseTokenDetails = testMaster.environment().baseToken;
        aliceErc20 = new zksync.Contract(tokenDetails.l2Address, zksync.utils.IERC20, alice);
    });

    test('Can burn and mint', async () => {
        if (process.env.CHAIN_ETH_ZKSYNC_NETWORK_ID != '320') {
            return;
        }
        const amount = ethers.utils.parseEther('1');
        const mintValue = ethers.utils.parseEther('2');

        const bridgehub = IBridgehubFactory.connect(L2_BRIDGEHUB_ADDRESS, alice.provider);
        const l2NativeTokenVault = IL1NativeTokenVaultFactory.connect(L2_NATIVE_TOKEN_VAULT_ADDRESS, alice.providerL1!);
        const assetId = await l2NativeTokenVault.getAssetId(ETH_ADDRESS_IN_CONTRACTS);

        const balanceBefore = await bobOtherChain.getBalance();

        const receipt = await bridgehub.requestL2TransactionTwoBridges(
            {
                chainId: '270',
                mintValue,
                l2Value: amount,
                l2GasLimit: 1000000,
                l2GasPerPubdataByteLimit: REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
                refundRecipient: alice.address,
                secondBridgeAddress: L2_ASSET_ROUTER_ADDRESS,
                secondBridgeValue: 0,
                secondBridgeCalldata: ethers.utils.concat([
                    ethers.utils.hexlify(1),
                    new ethers.utils.AbiCoder().encode(
                        ['bytes32', 'bytes'],
                        [
                            assetId,
                            new ethers.utils.AbiCoder().encode(['uint256', 'address'], [amount, await bob.getAddress()])
                        ]
                    )
                ])
            },
            { value: mintValue }
        );
        // submit tx on the destination
        // find receipt
        // get proof

        // submit tx
        const { l1BatchNumber, l2MessageIndex, l2TxNumberInBlock, message, proof } =
            await alice.finalizeWithdrawalParams(receipt.hash, 0);

        const balanceAfter = await bobOtherChain.getBalance();
        expect(balanceAfter).toEqual(balanceBefore.sub(amount));
    });
});

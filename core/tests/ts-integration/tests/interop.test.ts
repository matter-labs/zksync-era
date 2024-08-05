/**
 * This suite contains tests checking default ERC-20 contract behavior.
 */

import { TestMaster } from '../src/index';
// import { Token } from '../src/types';
// import { shouldChangeTokenBalances, shouldOnlyTakeFee } from '../src/modifiers/balance-checker';

import * as zksync from 'zksync-ethers';
// import {utils as interopUtils} from 'zksync-ethers-interop-support';
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
} from '../../../../contracts/l1-contracts/src.ts/constants';
import { RetryProvider } from '../src/retry-provider';
import { ETH_ADDRESS_IN_CONTRACTS } from 'zksync-ethers/build/utils';

const INTEROP_TX_TYPE = 253;

describe('Interop checks', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let bob: zksync.Wallet;
    let bobOtherChain: zksync.Wallet;
    // let tokenDetails: Token;
    // let baseTokenDetails: Token;
    // let aliceErc20: zksync.Contract;
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

        // tokenDetails = testMaster.environment().erc20Token;
        // baseTokenDetails = testMaster.environment().baseToken;
        // aliceErc20 = new zksync.Contract(tokenDetails.l2Address, zksync.utils.IERC20, alice);
    });

    test('Can burn and mint', async () => {
        // if (process.env.CHAIN_ETH_ZKSYNC_NETWORK_ID != '320') {
        //     return;
        // }
        const amount = ethers.utils.parseEther('1');
        // const mintValue = ethers.utils.parseEther('2');

        // const l1Bridgehub = IBridgehubFactory.connect(process.env.CONTRACTS_BRIDGEHUB_PROXY_ADDR!, alice.providerL1!);
        // const l1NativeTokenVault = IL1NativeTokenVaultFactory.connect(
        //     process.env.CONTRACTS_L1_NATIVE_TOKEN_VAULT_PROXY_ADDR!,
        //     alice.providerL1!
        // );
        // const assetId = await l1NativeTokenVault.getAssetId(ETH_ADDRESS_IN_CONTRACTS);
        const balanceBefore = await bobOtherChain.getBalance();

        // const tx = await l1Bridgehub.requestL2TransactionTwoBridges(
        //     {
        //         chainId: '270',
        //         mintValue,
        //         l2Value: amount,
        //         l2GasLimit: 1000000,
        //         l2GasPerPubdataByteLimit: REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
        //         refundRecipient: alice.address,
        //         secondBridgeAddress: L2_ASSET_ROUTER_ADDRESS,
        //         secondBridgeValue: 0,
        //         secondBridgeCalldata: ethers.utils.concat([
        //             ethers.utils.hexlify(1),
        //             new ethers.utils.AbiCoder().encode(
        //                 ['bytes32', 'bytes'],
        //                 [
        //                     assetId,
        //                     new ethers.utils.AbiCoder().encode(['uint256', 'address'], [amount, await bob.getAddress()])
        //                 ]
        //             )
        //         ])
        //     },
        //     { value: mintValue xw}
        // );

        // get proof
        // const { l1BatchNumber, l2MessageIndex, l2TxNumberInBlock, message, proof } =
        //     await alice.finalizeWithdrawalParams(tx.hash, 0);

        // console.log(l1BatchNumber, l2MessageIndex, l2TxNumberInBlock);
        // "tuple(tuple(address facet, uint8 action, bool isFreezable, bytes4[] selectors)[] facetCuts, address initAddress, bytes initCalldata)";

        const message = ethers.utils.defaultAbiCoder.encode(['uint256'], [amount]);
        const proof = ['0x'];
        const interopTx = {
            chainId: 270,
            to: await bob.getAddress(),
            from: L2_ASSET_ROUTER_ADDRESS,
            nonce: 0,
            calldata: message,
            customData: {
                paymaster_params: { paymaster: ethers.constants.AddressZero, paymasterInput: '0x' },
                merkleProof: proof,
                fullFee: '0xf000000000000000',
                toMint: ethers.utils.arrayify('0xf000000000000000000000000000000000')
            },
            maxFeePerGas: '0xf000000000000000',
            maxPriorityFeePerGas: '0xf000000000000000',
            gasLimit: '0xf000000000',
            type: INTEROP_TX_TYPE
        };
        /*
        This needs to be in zksync utils serialize
        if (meta.merkleProof) {
            fields.push(meta.merkleProof);
        }
        if (meta.fullFee) {
            fields.push(meta.fullFee);
        }
        if (meta.toMint) {
            fields.push(meta.toMint);
        }
        const txType = transaction.type || exports.EIP712_TX_TYPE;
        return ethers_1.utils.hexConcat([
            new Uint8Array([txType]),
            ethers_1.utils.RLP.encode(fields),
        ]); 
        */
        alice.provider.send('eth_sendTransaction', [zksync.utils.serialize(interopTx)]);
        console.log('kl todo serialized tx', zksync.utils.serialize(interopTx));
        // submit tx
        const balanceAfter = await bobOtherChain.getBalance();
        console.log('Balance before: ', balanceBefore.toString());
        console.log('Balance after: ', balanceAfter.toString());
        // expect(balanceAfter).toEqual(balanceBefore.sub(amount));
    });
});

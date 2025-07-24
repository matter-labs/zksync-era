/**
 * Collection of modifiers to check token balance changes caused by a transaction.
 */

import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { TestMessage } from '../matchers/matcher-helpers';
import { MatcherModifier, MatcherMessage } from '.';
import { Fee } from '../types';
import { getL2bUrl } from '../helpers';
import { IERC20__factory as IERC20Factory } from 'zksync-ethers/build/typechain';
import {
    ArtifactAssetTracker,
    ArtifactBridgeHub,
    ArtifactL1AssetRouter,
    ArtifactNativeTokenVault,
    L2_ASSET_TRACKER_ADDRESS,
    ArtifactInteropCenter
} from '../constants';
import { RetryProvider } from '../retry-provider';
import { getEcosystemContracts } from 'utils/build/tokens';
// checkout whole file before merge

/**
 * Modifier that ensures that fee was taken from the wallet for a transaction.
 * Note: if you need to check multiple wallets, it's better to use `shouldChangeETHBalances`
 * modifier, since it also includes the fee check.
 *
 * @param wallet Wallet that is expected to pay for a transaction.
 * @param isL1ToL2 Optional parameter that, if true, denotes that the checked transaction is an L1->L2 transaction.
 * @returns Matcher object
 */
export async function shouldOnlyTakeFee(wallet: zksync.Wallet, isL1ToL2?: boolean): Promise<ShouldChangeBalance> {
    return await ShouldChangeBalance.create(zksync.utils.ETH_ADDRESS, [{ wallet, change: 0n }], { l1ToL2: isL1ToL2 });
}

/**
 * Checks that the transaction caused ETH balance changes.
 * Balance changes may be both positive and negative.
 *
 * @param balanceChanges List of expected balance changes.
 * @param params Optional parameters (e.g. to disable the fee check or check balances on L1).
 * @returns Matcher object.
 */
export async function shouldChangeETHBalances(
    balanceChanges: BalanceChange[],
    params?: Params
): Promise<ShouldChangeBalance> {
    return await ShouldChangeBalance.create(zksync.utils.ETH_ADDRESS, balanceChanges, params);
}

/**
 * Checks that the transaction caused ETH balance changes.
 * Balance changes may be both positive and negative.
 *
 * @param token ERC20 token to check.
 * @param balanceChanges List of expected balance changes.
 * @param params Optional parameters (e.g. to disable the fee check or check balances on L1).
 * @returns Matcher object.
 */
export async function shouldChangeTokenBalances(
    token: string,
    balanceChanges: BalanceChange[],
    params?: Params
): Promise<ShouldChangeBalance> {
    return await ShouldChangeBalance.create(token, balanceChanges, {
        noAutoFeeCheck: true,
        l1: params?.l1 ?? false,
        ignoreUndeployedToken: params?.ignoreUndeployedToken ?? false
    });
}

/**
 * Represents an expected balance change in wei.
 * Change can be both positive and negative.
 * Hint: use `ethers.parseEther` for big amounts.
 *
 * If `addressToCheck` parameter is set, the balance would be checked
 * *for this provided address*. It may be very useful if you need to ensure the change
 * of balance for an account for which you can't create a `Wallet` object (e.g. custom
 * account or a certain smart contract).
 */
export interface BalanceChange {
    wallet: zksync.Wallet;
    change: bigint;
    addressToCheck?: string;
}

/**
 * Additional (optional) parameters to setup the balance change modifiers.
 */
export interface Params {
    noAutoFeeCheck?: boolean;
    l1?: boolean;
    l1ToL2?: boolean;
    ignoreUndeployedToken?: boolean;
    checkChainBalance?: boolean;
}

/**
 * Internal extension of `BalanceChange` that contains the balance value
 * *before* the transaction was sent.
 */
interface PopulatedBalanceChange extends BalanceChange {
    initialBalance: bigint;
    initialChainBalance: bigint;
}

/**
 * Generic modifier capable of checking for the balance change.
 * Can work with both ETH and ERC20 tokens, on L2 and L1.
 */
class ShouldChangeBalance extends MatcherModifier {
    token: string;
    balanceChanges: PopulatedBalanceChange[];
    noAutoFeeCheck: boolean;
    l1: boolean;
    l1ToL2: boolean;
    checkChainBalance: boolean;

    static async create(token: string, balanceChanges: BalanceChange[], params?: Params) {
        const l1 = params?.l1 ?? false;
        const noAutoFeeCheck = params?.noAutoFeeCheck ?? false;
        const l1ToL2 = params?.l1ToL2 ?? false;
        const checkChainBalance = params?.checkChainBalance ?? false;
        if (token == zksync.utils.ETH_ADDRESS && l1 && !noAutoFeeCheck) {
            throw new Error('ETH balance checks on L1 are not supported');
        }

        const populatedBalanceChanges: PopulatedBalanceChange[] = [];
        for (const entry of balanceChanges) {
            const wallet = entry.wallet;
            const address = entry.addressToCheck ?? entry.wallet.address;
            const initialBalance = await getBalance(l1, wallet, address, token, params?.ignoreUndeployedToken);
            const initialChainBalance = await getChainBalance(l1, wallet, token, params?.ignoreUndeployedToken);
            populatedBalanceChanges.push({
                wallet: entry.wallet,
                change: entry.change,
                addressToCheck: entry.addressToCheck,
                initialBalance,
                initialChainBalance
            });
        }

        return new ShouldChangeBalance(token, populatedBalanceChanges, noAutoFeeCheck, l1, l1ToL2, checkChainBalance);
    }

    private constructor(
        token: string,
        balanceChanges: PopulatedBalanceChange[],
        noAutoFeeCheck: boolean,
        l1: boolean,
        l1ToL2: boolean,
        checkChainBalance: boolean
    ) {
        super();
        this.token = token;
        this.balanceChanges = balanceChanges;
        this.noAutoFeeCheck = noAutoFeeCheck;
        this.l1 = l1;
        this.l1ToL2 = l1ToL2;
        this.checkChainBalance = checkChainBalance;
    }

    async check(receipt: zksync.types.TransactionReceipt): Promise<MatcherMessage | null> {
        let id = 0;
        for (const balanceChange of this.balanceChanges) {
            const prevBalance = balanceChange.initialBalance;
            const prevChainBalance = balanceChange.initialChainBalance;
            const wallet = balanceChange.wallet;
            const address = balanceChange.addressToCheck ?? balanceChange.wallet.address;
            let newBalance = await getBalance(this.l1, wallet, address, this.token);
            let newChainBalance = await getChainBalance(this.l1, wallet, this.token);
            // If fee should be checked, we're checking ETH token and this wallet is an initiator,
            // we should consider fees as well.
            const autoFeeCheck = !this.noAutoFeeCheck && this.token == zksync.utils.ETH_ADDRESS;
            if (autoFeeCheck) {
                // To "ignore" subtracted fee, we just add it back to the account balance.
                // For L1->L2 transactions the sender might be different from the refund recipient
                if (this.l1ToL2) {
                    newBalance = newBalance - extractRefundForL1ToL2(receipt, address);
                } else if (address == receipt.from) {
                    newBalance = newBalance + extractFee(receipt).feeAfterRefund;
                }
            }

            const diff = newBalance - prevBalance;
            const diffChainBalance = newChainBalance - prevChainBalance;
            if (this.checkChainBalance && !(await isMinterChain(this.l1, wallet, this.token))) {
                // console.log('diffChainBalance', diffChainBalance);
                if (diffChainBalance != diff && diffChainBalance + diff != 0n) {
                    // kl todo. We need this check. But it has issues. It does not query GW, only L1. And AssetTracker is not working properly on GW, as it does not check L1->L3 txs.
                    throw new Error(
                        `Chain balance change is not equal to the token balance change for wallet ${balanceChange.wallet.address} (index ${id} in array)`
                    );
                }
            }
            if (diff != balanceChange.change) {
                const message = new TestMessage()
                    .matcherHint(`ShouldChangeBalance modifier`)
                    .line(`Incorrect balance change for wallet ${balanceChange.wallet.address} (index ${id} in array)`)
                    .line(`Expected balance change to be:`)
                    .expected(balanceChange.change)
                    .line(`But actual change is:`)
                    .received(diff)
                    .line(`Balance before: ${prevBalance}, balance after: ${newBalance}`)
                    .build();

                return {
                    pass: false,
                    message: () => message
                };
            }

            id += 1;
        }

        return null;
    }
}

/**
 * Helper method to extract the fee in ETH wei from the transaction receipt.
 * Only works with L2 transactions.
 *
 * @param receipt Receipt of the transaction to extract fee from.
 * @param from Optional substitute to `receipt.from`.
 * @returns Extracted fee
 */
export function extractFee(receipt: zksync.types.TransactionReceipt, from?: string): Fee {
    from = from ?? receipt.from;

    const systemAccountAddress = '0x0000000000000000000000000000000000000000000000000000000000008001';
    // We need to pad address to represent 256-bit value.
    const fromAccountAddress = ethers.zeroPadValue(ethers.getBytes(from), 32);
    // Fee log is one that sends money to the system contract account.
    const feeLog = receipt.logs.find((log) => {
        return log.topics.length == 3 && log.topics[1] == fromAccountAddress && log.topics[2] == systemAccountAddress;
    });
    if (!feeLog) {
        throw {
            message: `No fee log was found in the following transaction receipt`,
            receipt
        };
    }

    const feeAmount = BigInt(feeLog.data);

    // There may be more than one refund log for the user
    const feeRefund = receipt.logs
        .filter((log) => {
            return (
                log.topics.length == 3 && log.topics[1] == systemAccountAddress && log.topics[2] == fromAccountAddress
            );
        })
        .map((log) => BigInt(log.data))
        .reduce((prev, cur) => {
            return prev + cur;
        }, 0n);

    return {
        feeBeforeRefund: feeAmount,
        feeAfterRefund: feeAmount - feeRefund,
        refund: feeRefund
    };
}

/**
 * Helper method to extract the refund for the L1->L2 transaction in ETH wei.
 *
 * @param receipt Receipt of the transaction to extract fee from.
 * @param from Optional substitute to `receipt.from`.
 * @returns Extracted fee
 */
function extractRefundForL1ToL2(receipt: zksync.types.TransactionReceipt, refundRecipient?: string): bigint {
    refundRecipient = refundRecipient ?? receipt.from;

    const mintTopic = ethers.keccak256(ethers.toUtf8Bytes('Mint(address,uint256)'));

    const refundLogs = receipt.logs.filter((log) => {
        return log.topics.length == 2 && log.topics[0] == mintTopic;
    });

    if (refundLogs.length === 0) {
        throw {
            message: `No refund log was found in the following transaction receipt`,
            receipt
        };
    }

    // Note, that it is important that the refund log is the last log in the receipt, because
    // there are multiple `Mint` events during a single L1->L2 transaction, so this one covers the
    // final refund.
    const refundLog = refundLogs[refundLogs.length - 1];

    const formattedRefundRecipient = ethers.hexlify(ethers.zeroPadValue(refundRecipient, 32));

    if (refundLog.topics[1].toLowerCase() !== formattedRefundRecipient.toLowerCase()) {
        throw {
            message: `The last ETH minted is not the refund recipient in the following transaction receipt`,
            receipt
        };
    }

    return BigInt(refundLog.data);
}

/**
 * Returns the balance of requested token for a certain address.
 *
 * @param l1 Whether to check l1 balance or l2
 * @param wallet Wallet to make requests from (may not represent the address to check)
 * @param address Address to check the balance
 * @param token Address of the token
 * @param ignoreUndeployedToken Whether allow token to be not deployed.
 *     If it's set to `true` and token is not deployed, then function returns 0.
 * @returns Token balance
 */
async function getBalance(
    l1: boolean,
    wallet: zksync.Wallet,
    address: string,
    token: string,
    ignoreUndeployedToken?: boolean
): Promise<bigint> {
    const provider = l1 ? wallet.providerL1! : wallet.provider;
    if (zksync.utils.isETH(token)) {
        return await provider.getBalance(address);
    } else {
        if (ignoreUndeployedToken && (await provider.getCode(token)) === '0x') {
            return 0n;
        }

        const erc20contract = IERC20Factory.connect(token, provider);
        return await erc20contract.balanceOf(address);
    }
}

/**
 * Returns the balance of requested token for a certain address.
 *
 * @param l1 Whether to check l1 balance or l2
 * @param wallet Wallet to make requests from (may not represent the address to check)
 * @param token Address of the token
 * @param ignoreUndeployedToken Whether allow token to be not deployed.
 *     If it's set to `true` and token is not deployed, then function returns 0.
 * @returns Token balance
 */
async function getChainBalance(
    l1: boolean,
    wallet: zksync.Wallet,
    token: string,
    ignoreUndeployedToken?: boolean
): Promise<bigint> {
    const provider = l1 ? wallet.providerL1! : wallet.provider;
    // kl todo get from env or something.

    const ecosystemContracts = await getEcosystemContracts(wallet);

    const settlementLayer = await ecosystemContracts.bridgehub.settlementLayer(
        (await wallet.provider.getNetwork()).chainId
    );

    const assetId = await ecosystemContracts.nativeTokenVault.assetId(token);

    // console.log("chainId", (await wallet.provider.getNetwork()).chainId, "assetId", assetId);
    let balance = await ecosystemContracts.assetTracker.chainBalance(
        (await wallet.provider.getNetwork()).chainId,
        assetId
    );
    // console.log('balance', l1 ? 'l1' : 'l2', balance);
    if (settlementLayer != (await wallet.providerL1!.getNetwork()).chainId && l1) {
        const gwProvider = new RetryProvider({ url: await getL2bUrl('gateway'), timeout: 1200 * 1000 }, undefined);
        const gwAssetTracker = new zksync.Contract(L2_ASSET_TRACKER_ADDRESS, ArtifactAssetTracker.abi, gwProvider);
        balance = await gwAssetTracker.chainBalance((await wallet.provider.getNetwork()).chainId, assetId);
    }
    return balance;
}

async function isMinterChain(l1: boolean, wallet: zksync.Wallet, token: string): Promise<boolean> {
    const bridgehub = new zksync.Contract(
        await (await wallet.getBridgehubContract()).getAddress(),
        ArtifactBridgeHub.abi,
        wallet.providerL1!
    );
    const assetRouter = new zksync.Contract(
        await bridgehub.assetRouter(),
        ArtifactL1AssetRouter.abi,
        wallet.providerL1!
    );
    const nativeTokenVault = new zksync.Contract(
        await assetRouter.nativeTokenVault(),
        ArtifactNativeTokenVault.abi,
        wallet.providerL1!
    );
    const assetId = await nativeTokenVault.assetId(token);
    // const assetTracker = new zksync.Contract(await bridgehub.assetTracker(), ArtifactAssetTracker.abi, wallet);
    // // return await assetTracker.isMinterChain( (await wallet.provider.getNetwork()).chainId, assetId);
    const provider = l1 ? wallet.providerL1! : wallet.provider;
    return (await nativeTokenVault.originChainId(assetId)) != (await provider.getNetwork()).chainId;
}

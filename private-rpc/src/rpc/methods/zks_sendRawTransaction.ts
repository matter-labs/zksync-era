// src/rpc/methods/send-raw-tx.ts
import { MethodHandler } from '@/rpc/rpc-service';
import { hexSchema } from '@/schemas/hex';
import { addressSchema } from '@/schemas/address';
import { isAddressEqual, parseTransaction, recoverTransactionAddress } from 'viem';
import { parseEip712Transaction } from 'viem/zksync';
import { invalidRequest, unauthorized } from '@/rpc/json-rpc';
import { delegateCall } from '@/rpc/delegate-call';
import { authorizer } from '@/permissions';
import { env } from '@/env';

/** Helper — understands *both* canonical RLP tx (types 0,1,2,3…) and
 *  zkSync’s EIP-712 tx (leading byte 0x71). */
function decodeTx(raw: `0x${string}`) {
    return raw.slice(2, 4).toLowerCase() === '71'
        ? parseEip712Transaction(raw) // zkSync EIP-712  :contentReference[oaicite:0]{index=0}
        : parseTransaction(raw); // Legacy / 2930 / 1559 / blob  :contentReference[oaicite:1]{index=1}
}

export const zks_sendRawTransactionWithDetailedOutput: MethodHandler = {
    name: 'zks_sendRawTransactionWithDetailedOutput',
    async handle(context, method, params, id) {
        if (env.PERMISSIONS_HOT_RELOAD == 'true') {
            authorizer.reloadFromEnv();
        }
        const rawTx = hexSchema.parse(params[0]);
        const tx = decodeTx(rawTx);

        console.log(tx.from, context.currentUser);
        const signer = tx.from ?? (await recoverTransactionAddress({ serializedTransaction: rawTx as any }));

        if (!isAddressEqual(signer, context.currentUser)) {
            return unauthorized(id, 'Cannot impersonate other users');
        }

        const txTo = addressSchema.safeParse(tx.to);
        if (!txTo.success) return invalidRequest(id);

        // (tx.data is present for both RLP & 0x71; paymaster/custom fields
        // live in tx.customData but are irrelevant for write-policy)
        if (tx.data && !context.authorizer.checkContractWrite(txTo.data, tx.data, context.currentUser)) {
            console.warn('Authorizer check failed', txTo.data, tx.data, context.currentUser);
            return unauthorized(id);
        }

        return delegateCall({
            url: context.targetRpcUrl,
            id,
            method,
            params: [rawTx]
        });
    }
};

export const eth_sendRawTransaction: MethodHandler = {
    ...zks_sendRawTransactionWithDetailedOutput,
    name: 'eth_sendRawTransaction'
};

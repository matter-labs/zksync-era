import { MethodHandler } from '@/rpc/rpc-service';
import { hexSchema } from '@/schemas/hex';
import { isAddressEqual, parseTransaction, recoverTransactionAddress } from 'viem';
import { invalidRequest, unauthorized } from '@/rpc/json-rpc';
import { addressSchema } from '@/schemas/address';
import { delegateCall } from '@/rpc/delegate-call';

export const zks_sendRawTransactionWithDetailedOutput: MethodHandler = {
    name: 'zks_sendRawTransactionWithDetailedOutput',
    async handle(context, method, params, id) {
        const rawTx = hexSchema.parse(params[0]);
        const tx = parseTransaction(rawTx);
        const address = await recoverTransactionAddress({
            serializedTransaction: rawTx as any
        });

        if (!isAddressEqual(address, context.currentUser)) {
            return unauthorized(id, 'Cannot impersonate other users');
        }

        const txTo = addressSchema.safeParse(tx.to);

        if (!txTo.success) {
            return invalidRequest(id);
        }

        if (tx.data && !context.authorizer.checkContractWrite(txTo.data, tx.data, context.currentUser)) {
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

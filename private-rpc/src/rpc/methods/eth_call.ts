import { z } from 'zod';
import { addressSchema } from '@/schemas/address';
import { hexSchema } from '@/schemas/hex';
import { MethodHandler } from '@/rpc/rpc-service';
import { isAddressEqual } from 'viem';
import { unauthorized } from '@/rpc/json-rpc';
import { delegateCall } from '@/rpc/delegate-call';
import { sendToTargetRpc } from '@/rpc/methods/utils';
import { authorizer } from '@/permissions';
import { env } from '@/env';

const callReqSchema = z
    .object({
        from: addressSchema.optional(),
        to: addressSchema,
        data: hexSchema.optional(),
        input: hexSchema.optional()
    })
    .passthrough();

const callResponseSchema = z.object({
    jsonrpc: z.literal('2.0'),
    id: z.any(),
    result: hexSchema.optional()
});

export const eth_call: MethodHandler = {
    name: 'eth_call',
    async handle(context, method, params, id) {
        if (env.PERMISSIONS_HOT_RELOAD == 'true') {
            authorizer.reloadFromEnv();
        }
        const call = callReqSchema.parse(params[0]);

        if (call.from !== undefined && !isAddressEqual(call.from, context.currentUser)) {
            return unauthorized(id);
        }

        const data = call.data || call.input;
        if (data && !context.authorizer.checkContractRead(call.to, data, context.currentUser)) {
            return unauthorized(id);
        }

        const rule = data && context.authorizer.checkPostReadFilter(call.to, data);
        if (rule) {
            const res = await sendToTargetRpc(context.targetRpcUrl, id, method, params, callResponseSchema);
            if (!res.result) {
                return unauthorized(id);
            }

            if (rule.canRead(context.currentUser, res.result)) {
                return res;
            } else {
                return unauthorized(id);
            }
        }

        return delegateCall({ url: context.targetRpcUrl, id, method, params });
    }
};

import { MethodHandler, RequestContext } from '@/rpc/rpc-service';
import { areHexEqual, sendToTargetRpc } from '@/rpc/methods/utils';
import { z } from 'zod';
import { FastifyReplyType } from 'fastify/types/type-provider';
import { addressSchema } from '@/schemas/address';
import { hexSchema } from '@/schemas/hex';
import { isAddressEqual, padHex } from 'viem';
import { unauthorized } from '@/rpc/json-rpc';

const schema = z
    .object({
        result: z
            .object({
                from: addressSchema,
                to: addressSchema,
                logs: z.array(z.object({ topics: z.array(hexSchema) }).passthrough())
            })
            .passthrough()
            .nullable()
    })
    .passthrough();

export const eth_getTransactionReceipt: MethodHandler = {
    name: 'eth_getTransactionReceipt',
    async handle(
        context: RequestContext,
        method: string,
        params: unknown[],
        id: number | string
    ): Promise<FastifyReplyType> {
        const data = await sendToTargetRpc(context.targetRpcUrl, id, method, params, schema);

        if (data.result === null) {
            return data;
        }

        const hasAccess =
            isAddressEqual(data.result.to, context.currentUser) ||
            isAddressEqual(data.result.from, context.currentUser) ||
            data.result.logs.some((log) => log.topics.some((t) => areHexEqual(padHex(context.currentUser), t)));

        if (!hasAccess) {
            return unauthorized(id);
        }

        return data;
    }
};

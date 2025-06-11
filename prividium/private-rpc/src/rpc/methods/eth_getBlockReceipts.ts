import { MethodHandler, RequestContext } from '@/rpc/rpc-service';
import { FastifyReplyType } from 'fastify/types/type-provider';
import { areHexEqual, sendToTargetRpc } from '@/rpc/methods/utils';
import { z } from 'zod';
import { addressSchema } from '@/schemas/address';
import { hexSchema } from '@/schemas/hex';
import { isAddressEqual, padHex } from 'viem';

const schema = z
    .object({
        result: z.array(
            z
                .object({
                    from: addressSchema,
                    to: addressSchema,
                    logs: z.array(
                        z
                            .object({
                                topics: z.array(hexSchema)
                            })
                            .passthrough()
                    )
                })
                .passthrough()
        )
    })
    .passthrough();

export const eth_getBlockReceipts: MethodHandler = {
    name: 'eth_getBlockReceipts',
    async handle(
        context: RequestContext,
        method: string,
        params: unknown[],
        id: number | string
    ): Promise<FastifyReplyType> {
        const data = await sendToTargetRpc(context.targetRpcUrl, id, method, params, schema);

        const filtered = data.result.filter(
            (receipt) =>
                isAddressEqual(receipt.to, context.currentUser) ||
                isAddressEqual(receipt.from, context.currentUser) ||
                receipt.logs.some((l) => l.topics.some((t) => areHexEqual(padHex(context.currentUser), t)))
        );

        return {
            ...data,
            result: filtered
        };
    }
};

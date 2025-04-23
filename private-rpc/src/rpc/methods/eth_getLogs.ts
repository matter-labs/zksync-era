import { MethodHandler, RequestContext } from '@/rpc/rpc-service';
import { FastifyReplyType } from 'fastify/types/type-provider';
import { z } from 'zod';
import { hexSchema } from '@/schemas/hex';
import { padHex } from 'viem';
import { areHexEqual, sendToTargetRpc } from '@/rpc/methods/utils';

const schema = z
    .object({
        jsonrpc: z.literal('2.0'),
        id: z.any(),
        result: z.array(
            z
                .object({
                    topics: z.array(hexSchema)
                })
                .passthrough()
        )
    })
    .passthrough();

export const eth_getLogs: MethodHandler = {
    name: 'eth_getLogs',
    async handle(
        context: RequestContext,
        method: string,
        params: unknown[],
        id: number | string
    ): Promise<FastifyReplyType> {
        const data = await sendToTargetRpc(context.targetRpcUrl, id, method, params, schema);

        const filtered = data.result.filter((log) =>
            log.topics.some((t) => areHexEqual(padHex(context.currentUser), t))
        );

        return {
            ...data,
            result: filtered
        };
    }
};

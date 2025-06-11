import { getAddress } from 'viem';
import { z } from 'zod';

export const addressSchema = z.string().transform((val, ctx) => {
    try {
        return getAddress(val);
    } catch {
        ctx.addIssue({
            code: z.ZodIssueCode.custom,
            message: 'Invalid address'
        });
        return z.NEVER;
    }
});

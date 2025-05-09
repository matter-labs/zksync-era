import { Hex } from 'viem';
import { z } from 'zod';

export const hexSchema = z
    .string()
    .regex(/^0x[0-9a-fA-F]*$/)
    .transform((hex) => hex as Hex);

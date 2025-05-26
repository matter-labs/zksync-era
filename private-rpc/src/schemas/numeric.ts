import { z } from "zod";

export const bigintStringSchema = z.string().refine((v) => {
  try {
    if (!/^[0-9]+$/.test(v)) {
      return false;
    }

    BigInt(v);
    return true;
  } catch {
    return false;
  }
});

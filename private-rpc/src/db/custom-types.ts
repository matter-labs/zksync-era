import { addressSchema } from '@/schemas/address';
import { hexSchema } from '@/schemas/hex';
import { customType } from 'drizzle-orm/pg-core';
import { Address, Hex } from 'viem';

export const hexRow = customType<{
  data: Hex;
  driverData: Buffer;
}>({
  dataType() {
    return 'bytea';
  },
  toDriver(val) {
    const parsed = hexSchema.parse(val);
    const hex = parsed.slice(2);
    // Avoid sending odd lengths. When length is odd Buffer.from ignores the first digit.
    const prefix = hex.length % 2 === 0 ? '' : '0';
    return Buffer.from(`${prefix}${hex}`, 'hex');
  },
  fromDriver(val) {
    const hex = `0x${val.toString('hex')}`;
    return hexSchema.parse(hex);
  },
});

export const addressRow = customType<{
  data: Address;
  driverData: Buffer;
}>({
  dataType() {
    return 'bytea';
  },
  toDriver(val) {
    const parsed = addressSchema.parse(val);
    const hex = parsed.slice(2);
    return Buffer.from(hex, 'hex');
  },
  fromDriver(val) {
    const address = `0x${val.toString('hex')}`;
    return addressSchema.parse(address);
  },
});

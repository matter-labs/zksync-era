# Test VM dumps

This directory contains VM dumps for regression testing.

## Overview

- [`validation_adjacent_storage_slots`](validation_adjacent_storage_slots.json): Accesses adjacent storage slots in a
  mapping during account validation (i.e., mapping values occupy >1 slot). Adjacent storage slots were previously
  disallowed for the fast VM.
- [`estimate_fee_for_transfer_to_self`](estimate_fee_for_transfer_to_self.json): Fee estimation step for a 0 base token
  transfer to self with a low gas limit. The legacy VM previously returned the incorrect revert reason.

# Test VM dumps

This directory contains VM dumps for regression testing.

## Overview

- [`validation_adjacent_storage_slots`](validation_adjacent_storage_slots.json): Accesses adjacent storage slots in a
  mapping during account validation (i.e., mapping values occupy >1 slot). Adjacent storage slots were previously
  disallowed for the fast VM.

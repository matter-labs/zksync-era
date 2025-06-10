#!/usr/bin/env python3
"""
check_handlers.py – RPC-coverage checker (namespace by namespace)

• Reads eth.rs and zks.rs for #[method(name = "...")]
• Reads rpc-method-handlers.ts for every eth_* / zks_* identifier
  – works whether it appears as a string literal **or** a bare variable
• Reports, per namespace:
    ⚠️  Missing handlers   – declared in Rust but absent in handlers.ts
    ⚠️  Extra (warning)    – present in handlers.ts but not declared in Rust
"""

from __future__ import annotations
import re
import sys
from pathlib import Path

# ─── Hard-coded file locations ──────────────────────────────────────────────
ETH_RS      = Path("../core/lib/web3_decl/src/namespaces/eth.rs")
ZKS_RS      = Path("../core/lib/web3_decl/src/namespaces/zks.rs")
HANDLERS_TS = Path("src/rpc/rpc-method-handlers.ts")
# ────────────────────────────────────────────────────────────────────────────

# Rust: #[method(name = "blockNumber")] → "blockNumber"
METHOD_RE = re.compile(r'#\s*\[method\([^)]*name\s*=\s*"([^"]+)"')

# TS:  eth_something   or   "eth_something"  or  'eth_something'
HANDLER_RE = re.compile(r'\b(eth_[A-Za-z0-9_]+|zks_[A-Za-z0-9_]+)\b')

def read_text(path: Path) -> str:
    try:
        return path.read_text()
    except FileNotFoundError:
        print(f"❌  File not found: {path.resolve()}")
        sys.exit(1)

def declared_with_prefix(rust_path: Path, prefix: str) -> set[str]:
    names = METHOD_RE.findall(read_text(rust_path))
    return {f"{prefix}{n}" for n in names}

def handler_names(prefix: str) -> set[str]:
    return {m for m in HANDLER_RE.findall(read_text(HANDLERS_TS)) if m.startswith(prefix)}

def report(ns: str, declared: set[str], handled: set[str]) -> None:
    missing = sorted(declared - handled)
    extras  = sorted(handled - declared)

    print(f"\n=== {ns.upper()} namespace ===")
    if missing:
        print(f"⚠️  Missing handlers in {HANDLERS_TS}:")
        print("\n".join(missing))
    else:
        print("✅  All declared methods are handled.")

    if extras:
        print(f"\n⚠️  Extra endpoints (missing in {ZKS_RS} or {ETH_RS}):")
        print("\n".join(extras))

def main() -> None:
    report("eth",
           declared_with_prefix(ETH_RS, "eth_"),
           handler_names("eth_"))

    report("zks",
           declared_with_prefix(ZKS_RS, "zks_"),
           handler_names("zks_"))

if __name__ == "__main__":
    main()

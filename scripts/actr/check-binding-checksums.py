#!/usr/bin/env python3
"""Verify a committed UniFFI binding against the checksums a library publishes.

The Kotlin SDK bindings are committed to this repository, but no pull-request job
ever loads a native library: the Kotlin job compiles the binding and runs
JVM-only unit tests, while the Android instrumentation test that would dlopen
libactr only runs in the scheduled E2E workflow. A stale checksum table
therefore stays invisible until an SDK consumer pairs the committed binding with
a library built from the same commit -- the moment UniFFI aborts inside
uniffiCheckApiChecksums and the whole SDK fails to load.

This script closes that gap. It calls the uniffi_actr_checksum_* symbols that a
freshly built library exports and compares each value with the constant
committed in the binding. Run it next to a "cargo build -p libactr".

Usage:
    python3 scripts/actr/check-binding-checksums.py \\
        --library target/debug/libactr.so \\
        --binding packages/actr-kotlin/actr-kotlin/src/main/kotlin/io/actrium/actr/actr.kt
"""

import argparse
import ctypes
import re
import sys
from pathlib import Path

# Matches the generated guard, e.g.
#   if (lib.uniffi_actr_checksum_func_actr_error_kind() != 40651.toShort()) {
CHECKSUM_CONST = re.compile(r"checksum_(func|method|constructor)_([A-Za-z0-9_]+)\(\) != (\d+)")

# Symbol names are embedded verbatim in the library, so the exported set can be
# scanned without a platform-specific tool such as nm.
CHECKSUM_SYMBOL = re.compile(rb"uniffi_actr_checksum_(func|method|constructor)_([A-Za-z0-9_]+)")


def binding_checksums(path: Path) -> dict[str, int]:
    found = CHECKSUM_CONST.findall(path.read_text())
    if not found:
        raise SystemExit(f"error: no UniFFI checksum constants found in {path}")
    return {f"{kind}_{name}": int(value) for kind, name, value in found}


def library_checksum_names(path: Path) -> set[str]:
    """Names of checksum symbols mentioned anywhere in the library image."""
    data = path.read_bytes()
    return {f"{kind.decode()}_{name.decode()}" for kind, name in CHECKSUM_SYMBOL.findall(data)}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binding", type=Path, required=True, help="committed binding source")
    parser.add_argument("--library", type=Path, required=True, help="freshly built cdylib/so/dylib")
    args = parser.parse_args()

    if not args.library.is_file():
        raise SystemExit(f"error: library not found: {args.library} (run: cargo build -p libactr)")

    expected = binding_checksums(args.binding)
    lib = ctypes.CDLL(str(args.library))

    mismatched: list[tuple[str, int, int]] = []
    unresolved: list[str] = []
    for key, committed in sorted(expected.items()):
        kind, name = key.split("_", 1)
        try:
            symbol = getattr(lib, f"uniffi_actr_checksum_{kind}_{name}")
        except AttributeError:
            unresolved.append(key)
            continue
        symbol.restype = ctypes.c_int16
        symbol.argtypes = []
        actual = symbol() & 0xFFFF
        if actual != committed:
            mismatched.append((key, committed, actual))

    absent = sorted(library_checksum_names(args.library) - set(expected))

    if unresolved:
        print("symbols missing from the library (binding is ahead of it):")
        for key in unresolved:
            print(f"  {key}")
    if mismatched:
        print("checksum drift (committed value != library value):")
        print(f"  {'symbol':<58}{'committed':>10}{'library':>10}")
        for key, committed, actual in mismatched:
            print(f"  {key:<58}{committed:>10}{actual:>10}")
    if absent:
        # Informational: the library may expose more than the committed binding
        # if the binding was generated from an older surface.
        print("checksum symbols the binding does not reference:")
        for key in absent:
            print(f"  {key}")

    if unresolved or mismatched:
        print()
        print(
            f"FAILED: {len(mismatched)} mismatched and {len(unresolved)} unresolved of "
            f"{len(expected)} checksums in {args.binding}."
        )
        print(
            "Regenerate the binding from a library built at this commit "
            "(packages/actr-kotlin/build-android.sh regenerates and patches it) "
            "instead of hand-editing the constants."
        )
        return 1

    print(f"OK: all {len(expected)} UniFFI checksums in {args.binding} match {args.library}")
    if absent:
        print(f"note: library also exposes {len(absent)} checksum symbols the binding omits")
    return 0


if __name__ == "__main__":
    sys.exit(main())

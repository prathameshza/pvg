#!/usr/bin/env python3
"""Generate collections.txt containing file paths and contents for LLM context."""

import os
import sys

# Directories/files to skip
SKIP_DIRS = {"target", ".git", "node_modules", "__pycache__", ".vscode", ".idea", "build"}
SKIP_EXT = {".exe", ".dll", ".png", ".jpg", ".ico", ".lock", ".svg"}
SKIP_FILES = {"LICENSE"}
MAX_FILE_SIZE = 500_000  # skip files larger than ~500KB

OUTPUT = "collections.txt"


def should_skip(path: str, name: str) -> bool:
    parts = path.replace("\\", "/").split("/")
    if any(p in SKIP_DIRS for p in parts):
        return True
    _, ext = os.path.splitext(name)
    if ext.lower() in SKIP_EXT or name in SKIP_FILES:
        return True
    return False


def main():
    root = os.path.dirname(os.path.abspath(__file__))
    out_path = os.path.join(root, OUTPUT)

    entries = []
    for dirpath, dirnames, filenames in os.walk(root):
        # prune skipped dirs in-place
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for name in filenames:
            full = os.path.join(dirpath, name)
            rel = os.path.relpath(full, root).replace("\\", "/")
            if should_skip(rel, name) or rel == OUTPUT:
                continue
            try:
                size = os.path.getsize(full)
                if size > MAX_FILE_SIZE:
                    print(f"Skipping (too large): {rel}")
                    continue
                with open(full, "r", encoding="utf-8", errors="replace") as f:
                    content = f.read()
                entries.append((rel, content))
            except OSError as e:
                print(f"Skipping (error): {rel}: {e}", file=sys.stderr)

    with open(out_path, "w", encoding="utf-8") as f:
        for rel, content in sorted(entries):
            f.write(f"===== FILE: {rel} =====\n")
            f.write("Below is this file's contents:\n")
            f.write(content)
            if not content.endswith("\n"):
                f.write("\n")
            f.write("\n")

    total_chars = sum(len(c) for _, c in entries)
    print(f"Wrote {len(entries)} files ({total_chars:,} chars) to {out_path}")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Fail unless an image index's children are the signed per-arch digests.

bootc pulls one architecture out of the index. The per-arch signatures
only cover that pull when those children are the digests that were signed.
"""

from __future__ import annotations

import json
import sys

ARCH = {
    "arm64": "aarch64",
    "aarch64": "aarch64",
    "amd64": "x86_64",
    "x86_64": "x86_64",
}


def digest_file(path: str) -> str:
    text = open(path, encoding="utf-8").read().strip()
    if len(text) != 71 or not text.startswith("sha256:"):
        sys.stderr.write(f"not a digest: {path}\n")
        raise SystemExit(1)
    return text


def main() -> int:
    if len(sys.argv) != 4:
        sys.stderr.write("usage: assert-index-children.py INDEX AARCH64 X86_64\n")
        return 2
    index = json.load(open(sys.argv[1], encoding="utf-8"))
    want = {
        "aarch64": digest_file(sys.argv[2]),
        "x86_64": digest_file(sys.argv[3]),
    }
    manifests = index.get("manifests")
    if not isinstance(manifests, list):
        sys.stderr.write("index has no manifest list\n")
        return 1
    seen: dict[str, str] = {}
    for item in manifests:
        if not isinstance(item, dict):
            sys.stderr.write("index manifest entry is not an object\n")
            return 1
        platform = item.get("platform")
        if not isinstance(platform, dict):
            sys.stderr.write("index manifest has no platform\n")
            return 1
        arch = ARCH.get(str(platform.get("architecture")))
        digest = item.get("digest")
        if arch is None or not isinstance(digest, str):
            sys.stderr.write(f"index child {platform.get('architecture')} {digest} is not a signed arch\n")
            return 1
        if digest != want[arch]:
            sys.stderr.write(f"index {arch} {digest} is not the signed digest {want[arch]}\n")
            return 1
        if arch in seen:
            sys.stderr.write(f"index lists {arch} twice\n")
            return 1
        seen[arch] = digest
    missing = [arch for arch in ("aarch64", "x86_64") if arch not in seen]
    if missing:
        sys.stderr.write(f"index is missing {', '.join(missing)}\n")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

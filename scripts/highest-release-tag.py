#!/usr/bin/env python3
"""Exit 0 when the given tag is the highest v* semver on stdin.

Stdin is either `git ls-remote --tags` output or one tag per line.
A release is higher than the same numbers with a prerelease. Tags that
are not vMAJOR.MINOR.PATCH with an optional prerelease are ignored.
"""

from __future__ import annotations

import re
import sys

TAG = re.compile(r"^v(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.]+))?$")


def version_key(tag: str) -> tuple | None:
    match = TAG.fullmatch(tag)
    if match is None:
        return None
    major, minor, patch = (int(match.group(i)) for i in range(1, 4))
    pre = match.group(4)
    if pre is None:
        pre_key: tuple = (1,)
    else:
        parts: list[tuple] = []
        for ident in pre.split("."):
            if ident.isdigit():
                parts.append((0, int(ident)))
            else:
                parts.append((1, ident))
        pre_key = (0, tuple(parts))
    return (major, minor, patch, pre_key)


def tag_name(line: str) -> str | None:
    text = line.strip()
    if not text:
        return None
    ref = text.split()[-1]
    if ref.endswith("^{}"):
        return None
    return ref.removeprefix("refs/tags/")


def main() -> int:
    if len(sys.argv) != 2:
        sys.stderr.write("usage: highest-release-tag.py VERSION\n")
        return 2
    version = sys.argv[1]
    want = version_key(version)
    if want is None:
        sys.stderr.write(f"not a version tag: {version}\n")
        return 2
    best = want
    best_tag = version
    for line in sys.stdin:
        name = tag_name(line)
        if name is None:
            continue
        key = version_key(name)
        if key is None or key <= best:
            continue
        best = key
        best_tag = name
    if best_tag != version:
        sys.stderr.write(f"{version} is older than {best_tag}\n")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

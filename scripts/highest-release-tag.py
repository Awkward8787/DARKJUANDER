#!/usr/bin/env python3
"""Exit 0 when VERSION is the highest stable v* tag on stdin.

Stdin is either `git ls-remote --tags` output or one tag per line.
A tag with a "-" suffix is a pre-release and is ignored. A component
with a leading zero is not a stable tag, so v1.02.0 does not rank as v1.2.0.
"""

from __future__ import annotations

import re
import sys

# Stable tags only. A "-" suffix is a pre-release and does not move :latest.
# A leading zero is not a stable component, so v1.02.0 is not v1.2.0.
STABLE = re.compile(r"^v(\d+)\.(\d+)\.(\d+)$")
PRERELEASE = re.compile(r"^v\d+\.\d+\.\d+-")


def version_key(tag: str) -> tuple | None:
    match = STABLE.fullmatch(tag)
    if match is None:
        return None
    parts: list[int] = []
    for group in match.groups():
        # v1.02.0 must not rank as v1.2.0.
        if len(group) > 1 and group.startswith("0"):
            return None
        parts.append(int(group))
    return tuple(parts)


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
        if PRERELEASE.match(version):
            sys.stderr.write(f"{version} is a pre-release; not moving :latest\n")
            return 1
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

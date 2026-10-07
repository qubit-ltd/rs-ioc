#!/usr/bin/env python3
"""Assert the manual cross-crate fixture does not enable qubit-ioc features."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
CARGO_TREE_COMMAND = [
    "cargo",
    "+1.94.0",
    "tree",
    "--manifest-path",
    "tests/fixtures/ioc_cross_crate/Cargo.toml",
    "-p",
    "qubit-ioc-fixture-manual",
    "-e",
    "normal",
    "--format",
    "{p}|{f}",
    "--depth",
    "1",
    "--prefix",
    "none",
    "--locked",
]


def parse_tree(output: str) -> str:
    """Return the sole direct qubit-ioc feature field, failing closed otherwise."""
    observed_lines = output.splitlines()
    matches: list[tuple[str, str]] = []
    for line in observed_lines:
        if "|" not in line:
            continue
        package, features = line.rsplit("|", 1)
        if package.startswith("qubit-ioc v"):
            matches.append((line, features))

    matching_lines = [line for line, _ in matches]
    if len(matches) != 1:
        raise ValueError(
            "expected exactly one direct qubit-ioc package row; "
            f"found {len(matches)}; observed rows: {matching_lines!r}"
        )

    line, features = matches[0]
    if features:
        raise ValueError(
            "manual fixture enabled qubit-ioc features; "
            f"observed row: {line!r}"
        )
    return features


def main() -> int:
    try:
        result = subprocess.run(
            CARGO_TREE_COMMAND,
            cwd=REPOSITORY_ROOT,
            check=True,
            capture_output=True,
            text=True,
        )
    except subprocess.CalledProcessError as error:
        print(
            "cargo tree failed with exit code "
            f"{error.returncode}: {error.stderr.strip()}",
            file=sys.stderr,
        )
        return 1

    try:
        parse_tree(result.stdout)
    except ValueError as error:
        print(f"manual feature isolation check failed: {error}", file=sys.stderr)
        return 1

    print("manual fixture has no enabled qubit-ioc features")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

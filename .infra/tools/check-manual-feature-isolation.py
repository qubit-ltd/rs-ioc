#!/usr/bin/env python3
"""Assert a cross-crate fixture does not enable qubit-ioc features."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_MANIFEST = "tests/fixtures/ioc_cross_crate/Cargo.toml"
DEFAULT_PACKAGE = "qubit-ioc-fixture-manual"


def build_tree_command(manifest_path: str, package: str) -> list[str]:
    return [
        "cargo", "+1.94.0", "tree", "--manifest-path", manifest_path,
        "-p", package, "-e", "normal", "--format", "{p}|{f}",
        "--depth", "1", "--prefix", "none", "--locked",
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


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Check resolved qubit-ioc features")
    parser.add_argument("--manifest-path", default=DEFAULT_MANIFEST)
    parser.add_argument("--package", default=DEFAULT_PACKAGE)
    args = parser.parse_args(argv)
    try:
        result = subprocess.run(
            build_tree_command(args.manifest_path, args.package),
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
        print(f"qubit-ioc feature isolation check failed: {error}", file=sys.stderr)
        return 1

    print(f"{args.package} has no enabled qubit-ioc features ({args.manifest_path})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

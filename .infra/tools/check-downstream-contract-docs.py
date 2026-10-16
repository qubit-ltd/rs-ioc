#!/usr/bin/env python3
"""Check consistency between current downstream manifests and their source notes."""

import argparse
import re
import sys
import tomllib
from pathlib import Path


DEPENDENCIES = (
    "qubit-ioc",
    "qubit-event-bus",
    "qubit-execution-services",
    "qubit-fs-registry",
    "qubit-fs-local",
    "qubit-fs",
)
FULL_SHA_RE = re.compile(r"(?<![0-9a-fA-F])[0-9a-fA-F]{40}(?![0-9a-fA-F])")
VERSION_STATEMENT_RE = re.compile(
    r"\bqubit-(?:ioc|event-bus)\b\s*(?:(?:version)\b\s*)?(?:[:=]\s*)?[\"']?(?:v)?\d+\.\d+(?:\.\d+)?(?:[-+][0-9A-Za-z.-]+)?[\"']?",
    re.IGNORECASE,
)
MARKDOWN_LINK_RE = re.compile(r"(?<!!)\[[^\]]*\]\(([^)]+)\)")


def _read_manifest(path, errors):
    try:
        return tomllib.loads(path.read_text(encoding="utf-8"))
    except OSError as error:
        errors.append(f"{path}: cannot read manifest: {error}")
    except (tomllib.TOMLDecodeError, UnicodeError) as error:
        errors.append(f"{path}: cannot parse TOML: {error}")
    return None


def _markdown_targets(source_text, source_path):
    targets = []
    for raw_target in MARKDOWN_LINK_RE.findall(source_text):
        stripped_target = raw_target.strip()
        if not stripped_target:
            continue
        target = stripped_target.split(maxsplit=1)[0].strip("<>")
        if not target or re.match(r"^[A-Za-z][A-Za-z0-9+.-]*:", target) or target.startswith("#"):
            continue
        path_part = target.split("#", 1)[0]
        is_relative = not Path(path_part).is_absolute() and not re.match(r"^[A-Za-z]:[\\/]", path_part)
        targets.append((path_part, (source_path.parent / path_part).resolve(), is_relative))
    return targets


def check(root: Path, source: Path, overlay: Path, native: Path) -> list[str]:
    """Return consistency errors for the supplied repository files."""
    errors = []
    overlay_data = _read_manifest(overlay, errors)
    native_data = _read_manifest(native, errors)
    versions = {}

    for path, manifest in ((overlay, overlay_data), (native, native_data)):
        if manifest is None:
            continue
        dependencies = manifest.get("dependencies", {})
        if not isinstance(dependencies, dict):
            errors.append(f"{path}: [dependencies] must be a table")
            continue
        for name in DEPENDENCIES:
            entry = dependencies.get(name)
            if not isinstance(entry, dict) or "version" not in entry:
                observed = "missing"
                errors.append(f"{path}: dependency {name} version is {observed}")
                versions.setdefault(name, {})[path] = observed
                continue
            value = entry["version"]
            versions.setdefault(name, {})[path] = str(value)

    for name, observed in versions.items():
        if len(observed) == 2 and len(set(observed.values())) != 1:
            details = ", ".join(f"{path}={value!r}" for path, value in observed.items())
            errors.append(f"dependency {name} version mismatch: {details}")

    try:
        source_text = source.read_text(encoding="utf-8")
    except OSError as error:
        errors.append(f"{source}: cannot read source document: {error}")
        return errors
    except UnicodeError as error:
        errors.append(f"{source}: cannot decode source document: {error}")
        return errors

    if FULL_SHA_RE.search(source_text):
        errors.append(f"{source}: contains a full 40-character SHA")
    for match in VERSION_STATEMENT_RE.finditer(source_text):
        errors.append(f"{source}: contains a direct version statement {match.group(0)!r}")

    targets = _markdown_targets(source_text, source)
    expected = {
        "current workflow": (root / ".github/workflows/downstream-contracts.yml").resolve(),
        "overlay manifest": overlay.resolve(),
    }
    for label, expected_path in expected.items():
        matching = [target for _, target, is_relative in targets if is_relative and target == expected_path]
        if not matching:
            errors.append(f"{source}: missing relative Markdown link to {label} ({expected_path})")
        elif not any(target.is_file() for target in matching):
            errors.append(f"{source}: Markdown link to {label} does not resolve to a file ({expected_path})")
    for raw_target, resolved, _ in targets:
        if not resolved.is_file():
            errors.append(f"{source}: Markdown link {raw_target!r} does not resolve to a file ({resolved})")
    return errors


def default_native_path(root: Path) -> Path:
    """Find the native consumer manifest beside a checkout or linked worktree."""
    relative_manifest = Path("rs-execution-services/tests/fixtures/ioc_application_consumer/Cargo.toml")
    fallback = root.parent / relative_manifest
    for ancestor in (root.parent, *root.parent.parents):
        candidate = ancestor / relative_manifest
        if candidate.is_file():
            return candidate
    return fallback


def main(argv=None):
    script = Path(__file__).resolve()
    root = script.parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=root / "tests/fixtures/application_consumer_current/SOURCE.md")
    parser.add_argument("--overlay", type=Path, default=root / "tests/fixtures/application_consumer_current/Cargo.toml")
    parser.add_argument("--native", type=Path, default=default_native_path(root))
    args = parser.parse_args(argv)

    errors = check(root, args.source, args.overlay, args.native)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("downstream contract documentation is consistent")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

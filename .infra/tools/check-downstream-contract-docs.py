#!/usr/bin/env python3
"""Check consistency between current downstream manifests and their source notes.

Different version requirements normalize only when both are stable versions with
one to three decimal components, an optional leading caret, and no leading
zeroes (except for the component ``0``). Other differing forms remain strict
mismatches; identical strings always match.
"""

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
    rf"(?<![A-Za-z0-9_-])`?(?:{'|'.join(map(re.escape, DEPENDENCIES))})`?"
    r"\s*(?:(?:version)\b\s*)?(?:[:=]\s*)?[\"']?(?:v)?"
    r"\d+\.\d+(?:\.\d+)?(?:[-+][0-9A-Za-z.-]+)?[\"']?",
    re.IGNORECASE,
)
MARKDOWN_LINK_RE = re.compile(r"(?<!!)\[[^\]]*\]\(([^)]+)\)")
STABLE_VERSION_REQUIREMENT_RE = re.compile(r"\^?(0|[1-9][0-9]*)(?:\.(0|[1-9][0-9]*))?(?:\.(0|[1-9][0-9]*))?\Z")


def equivalent_version_requirements(left: str, right: str) -> bool:
    """Compare identical requirements or equivalent bounded stable caret forms.

    Differing strings normalize only for stable one-to-three-component decimal
    forms, optionally prefixed by ``^``, with no leading zeroes except ``0``.
    All other differing forms remain mismatches.
    """
    if left == right:
        return True

    def bounds(requirement):
        match = STABLE_VERSION_REQUIREMENT_RE.fullmatch(requirement)
        if match is None:
            return None
        explicit = [int(part) for part in match.groups() if part is not None]
        lower = tuple(explicit + [0] * (3 - len(explicit)))
        upper = list(lower)
        first_nonzero = next((index for index, value in enumerate(explicit) if value != 0), None)
        if first_nonzero is None:
            # Zero versions use the precision of the written requirement.
            upper[len(explicit) - 1] += 1
        else:
            upper[first_nonzero] += 1
            for index in range(first_nonzero + 1, 3):
                upper[index] = 0
        return lower, tuple(upper)

    left_bounds = bounds(left)
    right_bounds = bounds(right)
    return left_bounds is not None and left_bounds == right_bounds


def _read_manifest(path, errors):
    try:
        return tomllib.loads(path.read_text(encoding="utf-8"))
    except OSError as error:
        errors.append(f"{path}: cannot read manifest: {error}")
    except (tomllib.TOMLDecodeError, UnicodeError) as error:
        errors.append(f"{path}: cannot parse TOML: {error}")
    return None


def _test_targets(manifest_path, manifest, errors, label):
    """Return Cargo test target names mapped to normalized source paths."""
    package = manifest.get("package", {})
    if not isinstance(package, dict):
        errors.append(f"{manifest_path}: [package] must be a table")
        package = {}
    targets = []
    if package.get("autotests", True) is not False:
        tests_dir = manifest_path.parent / "tests"
        if tests_dir.is_dir():
            try:
                targets.extend((path.stem, path) for path in sorted(tests_dir.glob("*.rs")))
                targets.extend((path.parent.name, path) for path in sorted(tests_dir.glob("*/main.rs")))
            except OSError as error:
                errors.append(f"{manifest_path}: cannot discover {label} test targets: {error}")

    explicit = manifest.get("test", [])
    if not isinstance(explicit, list):
        errors.append(f"{manifest_path}: [[test]] entries must be tables")
        explicit = []
    for index, entry in enumerate(explicit):
        if not isinstance(entry, dict):
            errors.append(f"{manifest_path}: [[test]] entry {index + 1} must be a table")
            continue
        name = entry.get("name")
        path = entry.get("path")
        if not isinstance(name, str) or not name:
            errors.append(f"{manifest_path}: [[test]] entry {index + 1} has no valid name")
            continue
        if path is None:
            path = f"tests/{name}.rs"
        elif not isinstance(path, str) or not path:
            errors.append(f"{manifest_path}: [[test]] target {name!r} has no valid path")
            continue
        targets.append((name, manifest_path.parent / path))

    by_name = {}
    for name, path in targets:
        normalized = path.resolve()
        by_name.setdefault(name, []).append(normalized)
        if not normalized.is_file():
            errors.append(f"{manifest_path}: {label} test target {name!r} source does not exist: {normalized}")
    for name, paths in by_name.items():
        if len(paths) > 1:
            rendered = ", ".join(str(path) for path in paths)
            errors.append(f"{manifest_path}: duplicate {label} test target name {name!r}: {rendered}")
    return {name: paths[0] for name, paths in by_name.items() if len(paths) == 1}


def _check_test_targets(overlay_path, overlay, native_path, native, errors):
    if overlay is None or native is None:
        return
    overlay_targets = _test_targets(overlay_path, overlay, errors, "wrapper")
    native_targets = _test_targets(native_path, native, errors, "native")
    for name in sorted(native_targets.keys() - overlay_targets.keys()):
        errors.append(f"wrapper is missing native test target {name!r} at {native_targets[name]}")
    for name in sorted(overlay_targets.keys() - native_targets.keys()):
        errors.append(f"wrapper has extra test target {name!r} at {overlay_targets[name]}")
    for name in sorted(native_targets.keys() & overlay_targets.keys()):
        if native_targets[name] != overlay_targets[name]:
            errors.append(
                f"test target {name!r} source route mismatch: native={native_targets[name]}, "
                f"wrapper={overlay_targets[name]}"
            )


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
    _check_test_targets(overlay, overlay_data, native, native_data, errors)
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
        if len(observed) == 2 and not equivalent_version_requirements(*observed.values()):
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

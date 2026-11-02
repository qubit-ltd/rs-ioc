#!/usr/bin/env python3
"""Check current package versions against their documented entry points."""

import re
import sys
import tomllib
from pathlib import Path


CURRENT_GUIDES = (
    "doc/user_guide.md",
    "doc/user_guide.zh_CN.md",
    "doc/complete-design.md",
    "doc/complete-design.zh_CN.md",
    "doc/lifecycle.md",
    "doc/lifecycle.zh_CN.md",
)
INSTALL_RE = re.compile(
    r"\bqubit-ioc\s*=\s*(?:\{[^}]*?\bversion\s*=\s*['\"]([^'\"]+)['\"]|['\"]([^'\"]+)['\"])",
    re.DOTALL,
)


def _read_text(path: Path, errors: list[str]) -> str | None:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        errors.append(f"{path}: cannot read file: {error}")
    except UnicodeError as error:
        errors.append(f"{path}: cannot decode UTF-8: {error}")
    return None


def _read_manifest(path: Path, errors: list[str]) -> dict | None:
    text = _read_text(path, errors)
    if text is None:
        return None
    try:
        return tomllib.loads(text)
    except tomllib.TOMLDecodeError as error:
        errors.append(f"{path}: invalid TOML: {error}")
        return None


def _package_version(path: Path, manifest: dict | None, errors: list[str]) -> str | None:
    if manifest is None:
        return None
    package = manifest.get("package")
    version = package.get("version") if isinstance(package, dict) else None
    if not isinstance(version, str) or not version:
        errors.append(f"{path}: missing [package].version declaration")
        return None
    return version


def _version_minor(version: str) -> str:
    pieces = version.split(".")
    return ".".join(pieces[:2]) if len(pieces) >= 2 else version


def _check_install(path: Path, text: str | None, expected_minor: str, errors: list[str]) -> None:
    if text is None:
        return
    matches = [first or second for first, second in INSTALL_RE.findall(text)]
    if not matches:
        errors.append(f"{path}: missing qubit-ioc installation version declaration")
        return
    for version in matches:
        if _version_minor(version) != expected_minor:
            errors.append(
                f"{path}: installation version expects minor {expected_minor}, found {version!r}"
            )


def check(root: Path) -> list[str]:
    """Return sorted diagnostics for the current package and documentation versions."""
    root = Path(root)
    errors = []
    runtime_path = root / "Cargo.toml"
    macros_path = root / "macros" / "Cargo.toml"
    runtime_manifest = _read_manifest(runtime_path, errors)
    macros_manifest = _read_manifest(macros_path, errors)
    runtime_version = _package_version(runtime_path, runtime_manifest, errors)
    macros_version = _package_version(macros_path, macros_manifest, errors)

    if runtime_manifest is not None and macros_version is not None:
        dependencies = runtime_manifest.get("dependencies", {})
        macro_dependency = dependencies.get("qubit-ioc-macros") if isinstance(dependencies, dict) else None
        required_version = macro_dependency.get("version") if isinstance(macro_dependency, dict) else None
        if not isinstance(required_version, str):
            errors.append(f"{runtime_path}: missing qubit-ioc-macros dependency version")
        elif required_version != macros_version:
            errors.append(
                f"{runtime_path}: qubit-ioc-macros dependency expects {macros_version}, found {required_version}"
            )

    if runtime_version is not None:
        for relative in CURRENT_GUIDES:
            path = root / relative
            text = _read_text(path, errors)
            if text is None:
                continue
            if relative.endswith("lifecycle.md"):
                migration_heading = "## Migration from 0.2 to 0.3"
            elif relative.endswith("lifecycle.zh_CN.md"):
                migration_heading = "## 从 0.2 迁移到 0.3"
            else:
                migration_heading = None

            if migration_heading is None:
                opening = "\n".join(text.splitlines()[:12])
            else:
                match = re.search(rf"(?m)^{re.escape(migration_heading)}\s*$", text)
                if match is None:
                    errors.append(f"{path}: missing expected migration section heading {migration_heading!r}")
                    opening = ""
                else:
                    opening = text[:match.start()]
            if runtime_version not in opening:
                errors.append(f"{path}: opening declaration is missing runtime version {runtime_version}")

        changelog_path = root / "CHANGELOG.md"
        changelog = _read_text(changelog_path, errors)
        if changelog is not None:
            first_heading = next((line.strip() for line in changelog.splitlines() if line.startswith("## ")), None)
            if first_heading != f"## {runtime_version}":
                errors.append(
                    f"{changelog_path}: latest section expects ## {runtime_version}, found {first_heading or 'no version heading'}"
                )

        expected_minor = _version_minor(runtime_version)
        for relative in ("README.md", "README.zh_CN.md", "macros/README.md", "macros/README.zh_CN.md"):
            path = root / relative
            _check_install(path, _read_text(path, errors), expected_minor, errors)

    if runtime_version is not None and macros_version is not None:
        statements = {
            "macros/README.md": (
                f"This checkout pairs qubit-ioc {runtime_version} with qubit-ioc-macros {macros_version}. "
                "Applications should use the runtime facade, which selects its macro dependency; "
                "the two packages do not need identical version numbers.",
            ),
            "macros/README.zh_CN.md": (
                f"本 checkout 使用 qubit-ioc {runtime_version} 与 qubit-ioc-macros {macros_version}。"
                "应用应通过运行时 facade 使用宏，由其依赖声明选择宏包；两个包不要求版本号相同。",
            ),
        }
        for relative, expected_statements in statements.items():
            path = root / relative
            text = _read_text(path, errors)
            if text is None:
                continue
            normalize = (lambda value: "".join(value.split())) if relative.endswith("zh_CN.md") else (lambda value: " ".join(value.split()))
            normalized = normalize(text)
            for statement in expected_statements:
                if normalized.count(normalize(statement)) < 2:
                    errors.append(
                        f"{path}: compatibility statement must appear twice with runtime {runtime_version} "
                        f"and macros {macros_version}"
                    )
                    break

    return sorted(errors)


def main() -> int:
    """Run the checker against this checkout and print stable diagnostics."""
    errors = check(Path(__file__).resolve().parents[2])
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("Documented package versions are consistent.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

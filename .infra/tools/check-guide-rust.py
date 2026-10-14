#!/usr/bin/env python3
"""Compile and run the Rust examples embedded in both user guides."""

from __future__ import annotations

import re
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import NamedTuple


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
GUIDES = (Path("doc/user_guide.md"), Path("doc/user_guide.zh_CN.md"))
TIMEOUT_SECONDS = 300
MARKER = re.compile(r"^\s*<!--\s*guide-rust:\s*(.*?)\s*-->\s*$")
FENCE = re.compile(r"^( {0,3})(`{3,}|~{3,})(.*)$")
ID_PATTERN = re.compile(r"^[a-z0-9_]+$")
EXPECTED = {
    "manual": ("run", "none"),
    "beans": ("run", "macros"),
    "typed_async": ("check", "none"),
    "scope": ("run", "none"),
    "replacement": ("check", "none"),
    "rollback": ("check", "none"),
    "cancellation": ("skip", None),
}


class Snippet(NamedTuple):
    id: str
    mode: str
    features: str | None
    reason: str | None
    code: str
    line: int
    path: Path


class GuideError(ValueError):
    """A guide parsing or bilingual-contract error with source location."""

    def __init__(self, path: Path, line: int, snippet_id: str, message: str):
        super().__init__(f"{path}:{line}: id={snippet_id}: {message}")


def _is_rust_fence(match: re.Match[str] | None) -> bool:
    if match is None:
        return False
    fence, info = match.group(2), match.group(3).strip()
    if fence.startswith("`") and "`" in info:
        return False
    language = info.split(maxsplit=1)[0].split(",", 1)[0] if info else ""
    return language.lower() == "rust"


def _closes_fence(match: re.Match[str] | None, opening_fence: str) -> bool:
    if match is None:
        return False
    closing_fence, trailing = match.group(2), match.group(3)
    return (
        closing_fence[0] == opening_fence[0]
        and len(closing_fence) >= len(opening_fence)
        and not trailing.strip()
    )


def _parse_marker(path: Path, line: int, raw: str) -> tuple[str, str, str | None, str | None]:
    fields: dict[str, str] = {}
    for item in raw.split():
        if "=" not in item:
            raise GuideError(path, line, "<unknown>", f"invalid marker field {item!r}")
        key, value = item.split("=", 1)
        if key not in {"id", "mode", "features", "reason"}:
            raise GuideError(path, line, fields.get("id", "<unknown>"), f"unknown field {key!r}")
        if key in fields:
            raise GuideError(path, line, fields.get("id", "<unknown>"), f"duplicate field {key!r}")
        fields[key] = value

    snippet_id = fields.get("id", "<missing>")
    if not ID_PATTERN.fullmatch(snippet_id):
        raise GuideError(path, line, snippet_id, "id must contain lowercase ASCII letters, digits, or underscores")
    mode = fields.get("mode", "")
    features = fields.get("features")
    reason = fields.get("reason")
    if mode not in {"run", "check", "skip"}:
        raise GuideError(path, line, snippet_id, f"invalid mode {mode!r}")
    if mode == "skip":
        if features is not None:
            raise GuideError(path, line, snippet_id, "skip markers must not define features")
        if not reason or not reason.strip():
            raise GuideError(path, line, snippet_id, "skip marker requires a non-empty reason")
    else:
        if features not in {"none", "macros"}:
            raise GuideError(path, line, snippet_id, f"invalid features {features!r}")
        if reason is not None:
            raise GuideError(path, line, snippet_id, "complete examples must not define reason")
    required = {"id", "mode"} | ({"reason"} if mode == "skip" else {"features"})
    if fields.keys() != required:
        missing = sorted(required - fields.keys())
        raise GuideError(path, line, snippet_id, f"missing required field(s): {', '.join(missing)}")
    return snippet_id, mode, features, reason


def parse_guide(path: Path, text: str) -> dict[str, Snippet]:
    """Parse marked Rust fences, rejecting unmarked and incomplete blocks."""
    lines = text.splitlines(keepends=True)
    snippets: dict[str, Snippet] = {}
    line_index = 0
    while line_index < len(lines):
        line_number = line_index + 1
        marker_match = MARKER.fullmatch(lines[line_index].rstrip("\r\n"))
        fence_match = FENCE.fullmatch(lines[line_index].rstrip("\r\n"))
        if marker_match:
            snippet_id, mode, features, reason = _parse_marker(path, line_number, marker_match.group(1))
            fence_index = line_index + 1
            next_fence = FENCE.fullmatch(lines[fence_index].rstrip("\r\n")) if fence_index < len(lines) else None
            if not _is_rust_fence(next_fence):
                raise GuideError(path, line_number, snippet_id, "marker must immediately precede a rust fence")
            opening = fence_index
            opening_fence = next_fence.group(2)
            code_lines: list[str] = []
            cursor = opening + 1
            while cursor < len(lines):
                closing = FENCE.fullmatch(lines[cursor].rstrip("\r\n"))
                if _closes_fence(closing, opening_fence):
                    break
                code_lines.append(lines[cursor])
                cursor += 1
            if cursor == len(lines):
                raise GuideError(path, opening + 1, snippet_id, "unclosed Rust fence")
            if snippet_id in snippets:
                raise GuideError(path, line_number, snippet_id, "duplicate ID")
            snippets[snippet_id] = Snippet(
                id=snippet_id,
                mode=mode,
                features=features,
                reason=reason,
                code="".join(code_lines),
                line=opening + 1,
                path=path,
            )
            line_index = cursor + 1
            continue
        if _is_rust_fence(fence_match):
            raise GuideError(path, line_number, "<unmarked>", "Rust fence has no adjacent guide-rust marker")
        if "guide-rust:" in lines[line_index]:
            raise GuideError(path, line_number, "<unknown>", "malformed guide-rust marker")
        line_index += 1
    return snippets


def validate_pair(en: dict[str, Snippet], zh: dict[str, Snippet]) -> None:
    """Require both guides to match the fixed, reviewable snippet inventory."""
    for label, snippets in (("English", en), ("Chinese", zh)):
        for snippet_id in EXPECTED.keys() - snippets.keys():
            raise GuideError(Path(f"{label} guide"), 1, snippet_id, "expected snippet is missing")
        for snippet_id in snippets.keys() - EXPECTED.keys():
            snippet = snippets[snippet_id]
            raise GuideError(snippet.path, snippet.line, snippet_id, "unexpected snippet ID")
        for snippet_id, expected in EXPECTED.items():
            snippet = snippets[snippet_id]
            observed = (snippet.mode, snippet.features)
            if observed != expected:
                raise GuideError(snippet.path, snippet.line, snippet_id, f"classification {observed!r} does not match fixed contract {expected!r}")

    for snippet_id in EXPECTED:
        english = en[snippet_id]
        chinese = zh[snippet_id]
        if (english.mode, english.features) != (chinese.mode, chinese.features):
            raise GuideError(chinese.path, chinese.line, snippet_id, "mode/features differ from English guide")


def cargo_command(snippet: Snippet, manifest_path: Path) -> list[str]:
    """Build the Rust 1.94 command for one complete guide example."""
    if snippet.mode == "skip":
        raise ValueError("skip snippets do not have Cargo commands")
    if snippet.mode == "run":
        action = ["run", "--quiet"]
    else:
        action = ["check", "--lib"]
    return ["cargo", "+1.94.0", *action, "--manifest-path", str(manifest_path)]


def _manifest(snippet: Snippet) -> str:
    features = ', features = ["macros"]' if snippet.features == "macros" else ""
    return (
        "[package]\nname = \"guide-example\"\nversion = \"0.0.0\"\n"
        "edition = \"2024\"\n\n[dependencies]\n"
        f"qubit-ioc = {{ path = \"{REPOSITORY_ROOT}\", default-features = false{features} }}\n"
    )


def check_snippet(snippet: Snippet) -> None:
    """Compile or run one snippet in an isolated temporary Cargo package."""
    if snippet.mode == "skip":
        return
    with tempfile.TemporaryDirectory(prefix="guide-rust-check-") as temporary:
        package = Path(temporary)
        source_dir = package / "src"
        source_dir.mkdir()
        source_name = "main.rs" if snippet.mode == "run" else "lib.rs"
        (package / "Cargo.toml").write_text(_manifest(snippet), encoding="utf-8")
        (source_dir / source_name).write_text(snippet.code, encoding="utf-8")
        manifest_path = package / "Cargo.toml"
        command = cargo_command(snippet, manifest_path)
        environment = dict(os.environ)
        environment["CARGO_TARGET_DIR"] = str(REPOSITORY_ROOT / "target/guide-rust-check")
        try:
            result = subprocess.run(
                command,
                cwd=REPOSITORY_ROOT,
                env=environment,
                check=False,
                capture_output=True,
                text=True,
                timeout=TIMEOUT_SECONDS,
            )
        except subprocess.TimeoutExpired as error:
            raise GuideError(
                snippet.path,
                snippet.line,
                snippet.id,
                f"command {command!r} timed out after {TIMEOUT_SECONDS}s\n"
                f"stdout:\n{error.stdout or ''}\nstderr:\n{error.stderr or ''}",
            ) from error
        except OSError as error:
            raise GuideError(
                snippet.path,
                snippet.line,
                snippet.id,
                f"could not start command {command!r}: {error}",
            ) from error
        if result.returncode != 0:
            raise GuideError(
                snippet.path,
                snippet.line,
                snippet.id,
                f"command {command!r} exited {result.returncode}\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}",
            )


def main() -> int:
    try:
        parsed = [
            parse_guide(path, (REPOSITORY_ROOT / path).read_text(encoding="utf-8"))
            for path in GUIDES
        ]
        validate_pair(*parsed)
        for snippets in parsed:
            for snippet in snippets.values():
                if snippet.mode == "skip":
                    print(f"SKIP {snippet.path}:{snippet.line} id={snippet.id}: {snippet.reason}")
                else:
                    check_snippet(snippet)
                    print(f"PASS {snippet.path}:{snippet.line} id={snippet.id} ({snippet.mode})")
    except (GuideError, OSError) as error:
        print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

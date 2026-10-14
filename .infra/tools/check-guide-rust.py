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
    if not _is_valid_fence_opening(match):
        return False
    info = match.group(3).strip()
    language = info.split(maxsplit=1)[0].split(",", 1)[0] if info else ""
    return language.lower() == "rust"


def _is_valid_fence_opening(match: re.Match[str] | None) -> bool:
    if match is None:
        return False
    fence, info = match.group(2), match.group(3)
    return not (fence.startswith("`") and "`" in info)


def _strip_quote_prefixes(line: str) -> tuple[str, int]:
    """Return a line's content and the number of explicit blockquote prefixes."""
    content = line.rstrip("\r\n")
    depth = 0
    while True:
        quote_prefix = re.match(r"^ {0,3}>[ \t]?", content)
        if quote_prefix is None:
            break
        depth += 1
        content = content[quote_prefix.end():]
    return content, depth


def _indent_after_quote_depth(line: str, quote_depth: int) -> int:
    """Return continuation indentation after stripping a list's quote ancestors."""
    content = line.rstrip("\r\n")
    for _ in range(quote_depth):
        quote = re.match(r"^ {0,3}>[ \t]?", content)
        if quote is None:
            break
        content = content[quote.end():]
    return len(content) - len(content.lstrip(" "))


def _line_list_prefixes(line: str) -> tuple[int, list[tuple[int, int, int]]]:
    """Return list contexts while preserving mixed quote/list prefix order."""
    remaining = line.rstrip("\r\n")
    quote_depth = 0
    offset = 0
    prefixes: list[tuple[int, int, int]] = []
    while True:
        quote = re.match(r"^ {0,3}>[ \t]?", remaining)
        if quote is not None:
            quote_depth += 1
            remaining = remaining[quote.end():]
            offset = 0
            continue
        item = re.match(r"^( *)([-+*]|[0-9]+[.)])([ ]+)", remaining)
        if item is None:
            break
        marker_indent = offset + len(item.group(1))
        content_indent = offset + item.end()
        prefixes.append((quote_depth, marker_indent, content_indent))
        offset += item.end()
        remaining = remaining[item.end():]
    return quote_depth, prefixes


def _strip_fence_container_prefixes(line: str) -> str:
    """Strip explicit quote and same-line list prefixes before fence matching."""
    content = line.rstrip("\r\n")
    while True:
        quote = re.match(r"^ {0,3}>[ \t]?", content)
        if quote is not None:
            content = content[quote.end():]
            continue
        item = re.match(r"^ {0,3}(?:[-+*]|[0-9]+[.)])[ \t]+", content)
        if item is not None:
            content = content[item.end():]
            continue
        return content


def _is_container_rust_fence(line: str) -> bool:
    """Identify Rust fences nested in blockquotes or list containers."""
    content = line.rstrip("\r\n")
    nested = False
    while True:
        quote_prefix = re.match(r"^ {0,3}>[ \t]?", content)
        if quote_prefix is not None:
            nested = True
            content = content[quote_prefix.end():]
            continue
        list_item = re.match(r"^ {0,3}(?:[-+*]|[0-9]+[.)])[ \t]+", content)
        if list_item is not None:
            nested = True
            content = content[list_item.end():]
            continue
        break

    if nested and _is_rust_fence(FENCE.fullmatch(content)):
        return True

    return False


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
    list_contexts: list[tuple[int, int, int]] = []
    enclosing_fence: tuple[str, int, tuple[tuple[int, int, int], ...]] | None = None
    line_index = 0
    while line_index < len(lines):
        line_number = line_index + 1
        source_line = lines[line_index].rstrip("\r\n")
        expanded_line = source_line.expandtabs(4)
        physical_indent = len(expanded_line) - len(expanded_line.lstrip(" "))
        source_fence = FENCE.fullmatch(source_line)
        logical_line, quote_depth = _strip_quote_prefixes(expanded_line)
        initial_indent = len(logical_line) - len(logical_line.lstrip(" "))
        normalized_source = expanded_line
        normalization_offset = 0
        if quote_depth == 0 and initial_indent:
            parents = [context for context in list_contexts if context[0] == 0]
            if parents and initial_indent >= parents[-1][2]:
                normalization_offset = parents[-1][2]
                normalized_source = logical_line[parents[-1][2]:]
        scanned_quote_depth, line_list_prefixes = _line_list_prefixes(normalized_source)
        logical_line, stripped_quote_depth = _strip_quote_prefixes(normalized_source)
        quote_depth = max(quote_depth, scanned_quote_depth, stripped_quote_depth)
        normalized_fence = FENCE.fullmatch(logical_line)
        if enclosing_fence is not None:
            delimiter, fence_quote_depth, fence_list_contexts = enclosing_fence
            container_exited = quote_depth < fence_quote_depth
            if logical_line.strip():
                for list_context in fence_list_contexts:
                    context_indent = _indent_after_quote_depth(expanded_line, list_context[0])
                    if context_indent < list_context[2]:
                        container_exited = True
                        break
            fence_match = source_fence or normalized_fence
            if container_exited:
                enclosing_fence = None
            else:
                if (
                    quote_depth == fence_quote_depth
                    and _is_valid_fence_opening(fence_match)
                    and _closes_fence(fence_match, delimiter)
                ):
                    enclosing_fence = None
                line_index += 1
                continue

        nested_rust_fence = _is_container_rust_fence(source_line) or _is_container_rust_fence(
            normalized_source
        )
        indent = len(logical_line) - len(logical_line.lstrip(" "))
        list_contexts = [context for context in list_contexts if context[0] <= quote_depth]
        matching_contexts = [context for context in list_contexts if context[0] == quote_depth]

        if logical_line.strip():
            while matching_contexts and _indent_after_quote_depth(
                expanded_line, matching_contexts[-1][0]
            ) < matching_contexts[-1][2]:
                obsolete = matching_contexts.pop()
                list_contexts.remove(obsolete)
            if matching_contexts and _indent_after_quote_depth(
                expanded_line, matching_contexts[-1][0]
            ) >= matching_contexts[-1][2]:
                content_indent = matching_contexts[-1][2]
                candidate = logical_line[content_indent:] if indent >= content_indent else logical_line
                nested_rust_fence = nested_rust_fence or _is_rust_fence(FENCE.fullmatch(candidate))
                nested_rust_fence = nested_rust_fence or _is_container_rust_fence(candidate)

        for prefix_quote_depth, marker_indent, content_indent in line_list_prefixes:
            if prefix_quote_depth == 0:
                marker_indent += normalization_offset
                content_indent += normalization_offset
            matching_contexts = [
                context for context in list_contexts if context[0] == prefix_quote_depth
            ]
            while matching_contexts and (
                marker_indent <= matching_contexts[-1][1]
                or marker_indent < matching_contexts[-1][2]
            ):
                obsolete = matching_contexts.pop()
                list_contexts.remove(obsolete)
            if marker_indent <= 3 or matching_contexts:
                context = (prefix_quote_depth, marker_indent, content_indent)
                matching_contexts.append(context)
                if context not in list_contexts:
                    list_contexts.append(context)

        if nested_rust_fence:
            raise GuideError(
                path,
                line_number,
                "<container>",
                "Rust fences inside blockquotes or indented Markdown containers are unsupported; move the fence to the document root",
            )
        marker_match = MARKER.fullmatch(source_line)
        fence_match = source_fence or normalized_fence or FENCE.fullmatch(
            _strip_fence_container_prefixes(normalized_source)
        )
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
        if _is_valid_fence_opening(fence_match):
            explicit_contexts = {
                (prefix_quote_depth, marker_indent, content_indent)
                for prefix_quote_depth, marker_indent, content_indent in line_list_prefixes
            }
            active_list_contexts = tuple(
                context
                for context in list_contexts
                if context in explicit_contexts
                or context[0] == quote_depth
                or physical_indent >= context[2]
            )
            enclosing_fence = (fence_match.group(2), quote_depth, active_list_contexts)
            line_index += 1
            continue
        if "guide-rust:" in source_line:
            raise GuideError(path, line_number, "<unknown>", "malformed guide-rust marker")
        line_index += 1
    return snippets


def validate_pair(
    en: dict[str, Snippet],
    zh: dict[str, Snippet],
    en_path: Path | None = None,
    zh_path: Path | None = None,
) -> None:
    """Require both guides to match the fixed, reviewable snippet inventory."""
    for label, snippets, guide_path in (
        ("English", en, en_path),
        ("Chinese", zh, zh_path),
    ):
        if guide_path is None:
            guide_path = next(iter(snippets.values())).path if snippets else Path(f"{label} guide")
        for snippet_id in EXPECTED.keys() - snippets.keys():
            raise GuideError(guide_path, 1, snippet_id, "expected snippet is missing")
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
        validate_pair(*parsed, *(REPOSITORY_ROOT / path for path in GUIDES))
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

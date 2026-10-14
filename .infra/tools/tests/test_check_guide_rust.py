import importlib.util
import subprocess
import unittest
from pathlib import Path
from unittest.mock import patch


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "check-guide-rust.py"
SPEC = importlib.util.spec_from_file_location("check_guide_rust", SCRIPT_PATH)
CHECKER = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(CHECKER)


EXPECTED = {
    "manual": ("run", "none"),
    "beans": ("run", "macros"),
    "typed_async": ("check", "none"),
    "scope": ("run", "none"),
    "replacement": ("check", "none"),
    "rollback": ("check", "none"),
    "cancellation": ("skip", None),
}


def guide(entries=None):
    entries = entries or EXPECTED
    blocks = []
    for snippet_id, (mode, features) in entries.items():
        attrs = f"id={snippet_id} mode={mode}"
        if features is not None:
            attrs += f" features={features}"
        if mode == "skip":
            attrs += " reason=application-provided-builder-and-signal"
        code = "fn main() {}" if mode == "run" else "pub fn example() {}"
        if mode == "skip":
            code = "builder.build(shutdown_signal)"
        blocks.extend([f"<!-- guide-rust: {attrs} -->", "```rust", code, "```"])
    return "\n".join(blocks) + "\n"


class ParseGuideTests(unittest.TestCase):
    def test_parses_run_check_and_skip_markers(self):
        parsed = CHECKER.parse_guide(Path("guide.md"), guide())
        self.assertEqual(set(parsed), set(EXPECTED))
        self.assertEqual((parsed["manual"].mode, parsed["manual"].features), ("run", "none"))
        self.assertEqual((parsed["typed_async"].mode, parsed["typed_async"].features), ("check", "none"))
        self.assertEqual(parsed["cancellation"].reason, "application-provided-builder-and-signal")
        self.assertEqual(parsed["manual"].code, "fn main() {}\n")

    def test_rejects_unmarked_rust_block_with_location(self):
        with self.assertRaises(CHECKER.GuideError) as caught:
            CHECKER.parse_guide(Path("guide.md"), "```rust\nfn main() {}\n```\n")
        self.assertIn("guide.md", str(caught.exception))
        self.assertIn(":1", str(caught.exception))
        self.assertIn("id", str(caught.exception))

    def test_rejects_unmarked_rust_blocks_with_long_or_extended_fences(self):
        cases = (
            "````rust\nfn main() {}\n````\n",
            "```rust,ignore\nfn main() {}\n```\n",
        )
        for content in cases:
            with self.subTest(content=content):
                try:
                    CHECKER.parse_guide(Path("guide.md"), content)
                except CHECKER.GuideError as error:
                    self.assertIn("guide.md:1", str(error))
                    self.assertIn("Rust fence has no adjacent", str(error))
                else:
                    self.fail("unmarked Rust fence was not rejected")

    def test_rejects_unmarked_rust_fences_in_quote_and_list_containers(self):
        containers = (
            "> ```rust\n> fn main() {}\n> ```\n",
            "- example:\n\n    ```rust\n    fn main() {}\n    ```\n",
            "- > ```rust\n  > fn main() {}\n  > ```\n",
            "> - ```rust\n>   fn main() {}\n>   ```\n",
            "- example:\n\n    > ```rust\n    > fn main() {}\n    > ```\n",
            "- outer\n    - ```rust\n      broken\n      ```\n",
            "> - example:\n>\n>     > ```rust\n>     > invalid Rust\n>     > ```\n",
            "- - example:\n\n      > ```rust\n      > invalid Rust\n      > ```\n",
            "- > - example:\n  >\n  >     ```rust\n  >     invalid Rust\n  >     ```\n",
            "- outer\n    > - inner\n    >     ```rust\n    >     broken\n    >     ```\n",
        )
        for container in containers:
            content = guide() + container
            with self.subTest(container=container):
                try:
                    CHECKER.parse_guide(Path("guide.md"), content)
                except CHECKER.GuideError as error:
                    self.assertIn("guide.md:", str(error))
                    self.assertIn("Rust fence", str(error))
                else:
                    self.fail("container Rust fence was not rejected")

    def test_preserves_indented_code_inside_blockquote(self):
        content = guide() + ">     ```rust\n>     invalid Rust\n>     ```\n"
        parsed = CHECKER.parse_guide(Path("guide.md"), content)
        self.assertEqual(set(parsed), set(EXPECTED))

    def test_ignores_markers_and_rust_fences_inside_text_fence(self):
        content = "````text\n" + guide() + "```rust\nfn main() {}\n```\n````\n"
        parsed = CHECKER.parse_guide(Path("guide.md"), content)
        self.assertEqual(parsed, {})
        with self.assertRaisesRegex(CHECKER.GuideError, "expected snippet is missing"):
            CHECKER.validate_pair(parsed, parsed, Path("guide.md"), Path("guide-zh.md"))

    def test_ignores_rust_looking_literal_inside_non_rust_fence(self):
        content = "```text\n```rust\nfn main() {}\n```\n```\n"
        parsed = CHECKER.parse_guide(Path("guide.md"), content)
        self.assertEqual(parsed, {})

    def test_rejects_rust_fence_after_invalid_backtick_info(self):
        content = guide() + "```text`inline`\n```rust\ninvalid Rust\n```\n"
        with self.assertRaisesRegex(CHECKER.GuideError, "Rust fence has no adjacent"):
            CHECKER.parse_guide(Path("guide.md"), content)

    def test_ignores_rust_literal_inside_quoted_non_rust_fence(self):
        content = (
            guide()
            + "> ````text\n> ```rust\n> literal Rust fence example\n> ```\n> `````\n"
        )
        parsed = CHECKER.parse_guide(Path("guide.md"), content)
        self.assertEqual(set(parsed), set(EXPECTED))

    def test_quote_exit_closes_non_rust_fence_context(self):
        content = guide() + "> ```text\n> example\n\n```rust\ninvalid Rust\n```\n"
        with self.assertRaisesRegex(CHECKER.GuideError, "Rust fence has no adjacent"):
            CHECKER.parse_guide(Path("guide.md"), content)

    def test_list_fence_context_and_exit(self):
        list_fence = "- ````text\n  ```rust\n  literal Rust fence example\n  ```\n  `````\n"
        parsed = CHECKER.parse_guide(Path("guide.md"), guide() + list_fence)
        self.assertEqual(set(parsed), set(EXPECTED))

        prefix = guide() + list_fence + "\n"
        root_fence_line = prefix.count("\n") + 1
        content = prefix + "```rust\ninvalid Rust\n```\n"
        with self.assertRaises(CHECKER.GuideError) as caught:
            CHECKER.parse_guide(Path("guide.md"), content)
        self.assertIn(f"guide.md:{root_fence_line}", str(caught.exception))

    def test_blank_line_does_not_exit_list_fence_context(self):
        content = (
            guide()
            + "- ````text\n\n  ```rust\n  literal Rust fence example\n  ```\n  ````\n"
        )
        parsed = CHECKER.parse_guide(Path("guide.md"), content)
        self.assertEqual(set(parsed), set(EXPECTED))

    def test_list_markers_inside_fence_do_not_become_closers(self):
        content = (
            guide()
            + "````text\n- `````\n```rust\nliteral Rust fence example\n```\n````\n"
        )
        parsed = CHECKER.parse_guide(Path("guide.md"), content)
        self.assertEqual(set(parsed), set(EXPECTED))

    def test_list_quote_fence_context_exits_before_root_quote_rust(self):
        content = (
            guide()
            + "- > ```text\n  > literal\n> ```rust\n> invalid Rust\n> ```\n"
        )
        with self.assertRaisesRegex(CHECKER.GuideError, "Rust fences inside blockquotes"):
            CHECKER.parse_guide(Path("guide.md"), content)

    def test_list_between_nested_quotes_preserves_fence_context(self):
        content = (
            guide()
            + "> - > ````text\n"
            + ">   > ```rust\n>   > literal\n>   > ```\n>   > ````\n"
        )
        parsed = CHECKER.parse_guide(Path("guide.md"), content)
        self.assertEqual(set(parsed), set(EXPECTED))

    def test_list_continuation_fence_exits_before_root_rust(self):
        content = guide() + "- item\n  ```text\n  literal\n\n```rust\ninvalid Rust\n```\n"
        with self.assertRaisesRegex(CHECKER.GuideError, "Rust fence has no adjacent"):
            CHECKER.parse_guide(Path("guide.md"), content)

    def test_exiting_quote_closes_its_list_context(self):
        content = (
            guide()
            + "> - example:\n>\noutside quote\n\n"
            + ">     ```rust\n>     invalid Rust\n>     ```\n"
        )
        parsed = CHECKER.parse_guide(Path("guide.md"), content)
        self.assertEqual(set(parsed), set(EXPECTED))

    def test_parses_marked_long_and_extended_rust_fences(self):
        cases = (
            ("````rust\nfn main() {}\n```\n````\n", "fn main() {}\n```\n"),
            ("```rust,ignore\nfn main() {}\n```\n", "fn main() {}\n"),
        )
        for fence, expected_code in cases:
            content = f"<!-- guide-rust: id=manual mode=run features=none -->\n{fence}"
            with self.subTest(fence=fence):
                parsed = CHECKER.parse_guide(Path("guide.md"), content)
                self.assertEqual(parsed["manual"].code, expected_code)

    def test_rejects_duplicate_id(self):
        content = guide({"manual": EXPECTED["manual"]}) + guide({"manual": EXPECTED["manual"]})
        with self.assertRaisesRegex(CHECKER.GuideError, "id=manual: duplicate ID"):
            CHECKER.parse_guide(Path("guide.md"), content)

    def test_rejects_dangling_marker_unknown_field_and_invalid_values(self):
        cases = (
            ("<!-- guide-rust: id=manual mode=run features=none -->\ntext\n", "manual"),
            ("<!-- guide-rust: id=manual mode=run features=none extra=x -->\n```rust\nx\n```\n", "manual"),
            ("<!-- guide-rust: id=manual mode=maybe features=none -->\n```rust\nx\n```\n", "manual"),
            ("<!-- guide-rust: id=manual mode=run features=bogus -->\n```rust\nx\n```\n", "manual"),
        )
        for content, snippet_id in cases:
            with self.subTest(content=content), self.assertRaises(CHECKER.GuideError) as caught:
                CHECKER.parse_guide(Path("guide.md"), content)
            self.assertIn("guide.md:1", str(caught.exception))
            self.assertIn(f"id={snippet_id}", str(caught.exception))

    def test_rejects_unclosed_fence_and_empty_skip_reason(self):
        cases = (
            ("<!-- guide-rust: id=manual mode=run features=none -->\n```rust\nfn main() {}\n", "manual", 2),
            ("<!-- guide-rust: id=cancellation mode=skip reason= -->\n```rust\nx\n```\n", "cancellation", 1),
        )
        for content, snippet_id, line in cases:
            with self.subTest(content=content), self.assertRaises(CHECKER.GuideError) as caught:
                CHECKER.parse_guide(Path("guide.md"), content)
            self.assertIn(f"guide.md:{line}", str(caught.exception))
            self.assertIn(f"id={snippet_id}", str(caught.exception))

    def test_fixed_full_example_cannot_be_skipped(self):
        altered = dict(EXPECTED)
        altered["manual"] = ("skip", None)
        with self.assertRaisesRegex(CHECKER.GuideError, "manual"):
            CHECKER.validate_pair(
                CHECKER.parse_guide(Path("en.md"), guide(altered)),
                CHECKER.parse_guide(Path("zh.md"), guide()),
            )

    def test_pair_requires_same_ids_modes_and_features(self):
        english = CHECKER.parse_guide(Path("en.md"), guide())
        entries = dict(EXPECTED)
        entries["manual"] = ("check", "none")
        with self.assertRaisesRegex(CHECKER.GuideError, "manual"):
            CHECKER.validate_pair(english, CHECKER.parse_guide(Path("zh.md"), guide(entries)))

    def test_pair_accepts_matching_fixed_inventory(self):
        CHECKER.validate_pair(
            CHECKER.parse_guide(Path("en.md"), guide()),
            CHECKER.parse_guide(Path("zh.md"), guide()),
        )

    def test_missing_fixed_id_diagnostic_uses_guide_path(self):
        entries = dict(EXPECTED)
        del entries["manual"]
        english_path = Path("/docs/user_guide.md")
        chinese_path = Path("/docs/user_guide.zh_CN.md")
        with self.assertRaises(CHECKER.GuideError) as caught:
            CHECKER.validate_pair(
                CHECKER.parse_guide(english_path, guide()),
                CHECKER.parse_guide(chinese_path, guide(entries)),
                english_path,
                chinese_path,
            )
        self.assertIn("/docs/user_guide.zh_CN.md:1", str(caught.exception))
        self.assertIn("id=manual", str(caught.exception))


class CargoCommandTests(unittest.TestCase):
    def test_cargo_run_and_check_commands(self):
        snippets = CHECKER.parse_guide(Path("guide.md"), guide())
        manifest = Path("/tmp/example/Cargo.toml")
        run = CHECKER.cargo_command(snippets["manual"], manifest)
        check = CHECKER.cargo_command(snippets["typed_async"], manifest)
        self.assertEqual(run, ["cargo", "+1.94.0", "run", "--quiet", "--manifest-path", str(manifest)])
        self.assertEqual(check, ["cargo", "+1.94.0", "check", "--lib", "--manifest-path", str(manifest)])
        self.assertIn('path = "' + str(CHECKER.REPOSITORY_ROOT) + '"', CHECKER._manifest(snippets["manual"]))
        self.assertIn('default-features = false', CHECKER._manifest(snippets["manual"]))
        self.assertNotIn('features = ["macros"]', CHECKER._manifest(snippets["manual"]))
        self.assertIn('features = ["macros"]', CHECKER._manifest(snippets["beans"]))
        with self.assertRaisesRegex(ValueError, "skip"):
            CHECKER.cargo_command(snippets["cancellation"], manifest)

    def test_command_start_failure_has_location_and_command(self):
        snippet = CHECKER.Snippet("manual", "run", "none", None, "fn main() {}\n", 12, Path("guide.md"))
        with patch.object(CHECKER.subprocess, "run", side_effect=FileNotFoundError("cargo missing")):
            with self.assertRaises(CHECKER.GuideError) as caught:
                CHECKER.check_snippet(snippet)
        diagnostic = str(caught.exception)
        self.assertIn("guide.md:12", diagnostic)
        self.assertIn("id=manual", diagnostic)
        self.assertIn("cargo", diagnostic)
        self.assertIn("could not start command", diagnostic)

    def test_skip_snippet_does_not_start_subprocess(self):
        snippet = CHECKER.Snippet("cancellation", "skip", None, "application-context", "builder", 8, Path("guide.md"))
        with patch.object(CHECKER.subprocess, "run") as run:
            CHECKER.check_snippet(snippet)
        run.assert_not_called()

    def test_nonzero_and_timeout_diagnostics_include_output_and_cleanup(self):
        snippet = CHECKER.Snippet("typed_async", "check", "none", None, "pub fn example() {}\n", 23, Path("zh.md"))
        failure = subprocess.CompletedProcess([], 101, stdout="compiler output", stderr="compiler error")
        for result, expected in (
            (failure, ("exited 101", "compiler output", "compiler error")),
            (
                subprocess.TimeoutExpired([], 300, output="partial stdout", stderr="partial stderr"),
                ("timed out", "partial stdout", "partial stderr"),
            ),
        ):
            with self.subTest(expected=expected):
                patch_args = {"side_effect": result} if isinstance(result, BaseException) else {"return_value": result}
                with patch.object(CHECKER.subprocess, "run", **patch_args) as run:
                    with self.assertRaises(CHECKER.GuideError) as caught:
                        CHECKER.check_snippet(snippet)
                diagnostic = str(caught.exception)
                self.assertIn("zh.md:23", diagnostic)
                self.assertIn("id=typed_async", diagnostic)
                self.assertIn("cargo", diagnostic)
                for part in expected:
                    self.assertIn(part, diagnostic)
                manifest_path = Path(run.call_args.args[0][-1])
                self.assertFalse(manifest_path.parent.exists())


if __name__ == "__main__":
    unittest.main()

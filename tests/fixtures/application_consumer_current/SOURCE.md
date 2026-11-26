# Current downstream consumer

This wrapper builds the current consumer from the sibling `rs-execution-services`
checkout. It uses the explicit library name `ioc_application_consumer` and
registers all five public integration-test files, including the six-test mixed
execution-domain rollback suite. The wrapper keeps `qubit-ioc` default features
disabled and checks that its direct feature set stays empty.

## Source inputs

The CI source-preparation manifest is
[`../../../.infra/ci/local-path-dependencies.tsv`](../../../.infra/ci/local-path-dependencies.tsv).
It selects the `dev-starfish` branch for sibling source preparation. The
candidate SHAs recorded in its comments and verified for this lane are:

| Checkout | Revision | Role |
| --- | --- | --- |
| `rs-fs-registry` | `be7c9c956829d8b48061b78f02b9a04972a61f54` | `qubit-fs-registry` 0.8.3 |
| `rs-fs` | `4cb4ac38f993efb99d52b16ead8aef5f72cfef49` | `qubit-fs` 0.4.0 |
| `rs-fs-local` | `3bf4275ed601e39fdbf8532f9d80d563d9cc7182` | `qubit-fs-local` 0.12.0, with `registry` enabled |
| `rs-local-files` | `80d4357b354f572e695f0c17ac1182f0aaad22c0` | Transitive local filesystem provider used by `rs-fs-local` |
| `rs-execution-services` | `e822fa63080c724aaf1ceb21cb73a0f271d5f86b` | Current consumer library, binary, and integration tests |
| `rs-executor` | `1c543c4b2fd08d8e9fd04d8be7499c122c01be37` | Path dependency of the selected source graph |

The overlay manifest records the relative paths and version requirements. Its
lockfile is [`Cargo.lock`](Cargo.lock). The workflow
[`downstream-contracts.yml`](../../../.github/workflows/downstream-contracts.yml)
checks out the listed candidate SHAs beside `rs-ioc`, then runs the same locked
commands shown below. Local verification uses the registered sibling layout;
the developer updates the sibling worktrees to the selected commits and records
their SHAs. A local run does not establish that remote CI passed.

## T1 baseline evidence

Before the T6 update, the required locked metadata command exited 101 because
the selected local provider was 0.12.0 while this overlay required
`qubit-fs-local ^0.11`:

```text
cargo +1.94.0 metadata --locked --all-features --format-version 1 --manifest-path tests/fixtures/application_consumer_current/Cargo.toml
error: failed to select a version for the requirement `qubit-fs-local = "^0.11"`
candidate versions found which didn't match: 0.12.0
```

## Reproduction

Run these commands from the `rs-ioc` checkout with the source revisions above
checked out beside it. Keep a separate Cargo target directory for this lane.

```sh
cargo +1.94.0 metadata --locked --all-features --format-version 1 --manifest-path tests/fixtures/application_consumer_current/Cargo.toml
cargo +1.94.0 check --locked --all-targets --manifest-path tests/fixtures/application_consumer_current/Cargo.toml
cargo +1.94.0 test --locked --manifest-path tests/fixtures/application_consumer_current/Cargo.toml
cargo +1.94.0 run --locked --manifest-path tests/fixtures/application_consumer_current/Cargo.toml
python3 .infra/tools/check-manual-feature-isolation.py --manifest-path tests/fixtures/application_consumer_current/Cargo.toml --package ioc-downstream-consumer-current
```

Metadata should resolve the consumer, IoC, registry, core filesystem, and local
provider to the `rs-ioc`, `rs-fs-registry`, `rs-fs`, and `rs-fs-local` checkouts
listed above. The consumer and local provider both resolve the same
`qubit-fs-registry` 0.8.3 package ID from `rs-fs-registry`; the consumer and
local provider also share `qubit-fs` 0.4.0 from `rs-fs`. Preserve the full
machine-specific package IDs with CI or local verification results.

## Source checkout details retained from the existing current lane

This wrapper compiles the current sibling `rs-execution-services` consumer's
library, binary, and five public integration-test files. It uses the explicit
library name `ioc_application_consumer`.

Its lockfile is [`Cargo.lock`](Cargo.lock); the native
consumer has a separate lockfile in its own checkout, used by the workflow's
native locked test command.

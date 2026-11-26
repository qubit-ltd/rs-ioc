# Current downstream consumer

This wrapper compiles the current sibling `rs-execution-services` consumer's
library, binary, and five public integration-test files. It uses the explicit
library name `ioc_application_consumer`; consumer source is not copied into this
historical fixture. The current lane disables `qubit-ioc` default features and
checks that the resolved direct feature set remains empty.

The [downstream workflow](../../../.github/workflows/downstream-contracts.yml)
prepares the sibling repositories and verifies their pinned revisions before
running the wrapper. The fixture manifest uses committed relative paths for
each local source checkout, including the current adapter candidate at
`../../../../rs-fs-local`; the registry feature remains enabled. The wrapper
reads the current consumer source from the sibling execution-services
checkout. The candidate filesystem source revisions are recorded in that
workflow. Its lockfile is [`Cargo.lock`](Cargo.lock). A local checkout or
successful local run does not establish that a remote workflow revision
passed.
This wrapper lockfile is aligned with the current lane's pinned sibling
manifests and registry versions; keep it synchronized when those pins change.

The wrapper registers five external integration-test targets, including the
mixed execution-domain rollback suite with six regression tests. Every target
must be present and all its tests must be discovered and run. Zero-test unit
targets are expected. Run the feature-isolation check and locked metadata,
check, test, and run commands from the `rs-ioc` checkout. Cargo test executes
the five explicitly registered external targets:

```sh
python3 .infra/tools/check-manual-feature-isolation.py --manifest-path tests/fixtures/application_consumer_current/Cargo.toml --package ioc-downstream-consumer-current
cargo +1.94.0 metadata --manifest-path tests/fixtures/application_consumer_current/Cargo.toml --locked --format-version 1
cargo +1.94.0 check --manifest-path tests/fixtures/application_consumer_current/Cargo.toml --locked
cargo +1.94.0 test --manifest-path tests/fixtures/application_consumer_current/Cargo.toml --locked
cargo +1.94.0 run --manifest-path tests/fixtures/application_consumer_current/Cargo.toml --locked
```

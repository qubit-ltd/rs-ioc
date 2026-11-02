# Current downstream consumer

This wrapper compiles the current sibling `rs-execution-services` consumer's
library, binary, and four public integration-test files. It uses the explicit
library name `ioc_application_consumer`; consumer source is not copied into this
historical fixture. Both consumers disable `qubit-ioc` default features, and the
current lane checks that the resolved direct feature set remains empty.

The current workflow selects the sibling checkout revisions and source-routing
details. The [workflow](../../../.github/workflows/downstream-contracts.yml)
and [overlay manifest](Cargo.toml) are the source of truth for those choices.
The wrapper reads the current consumer source from the sibling checkout at the
selected revision. Its lockfile is
[`Cargo.lock`](Cargo.lock); the native consumer has a separate lockfile in its
own checkout, used by the workflow's native locked test command. A local
checkout or successful local run does not establish that a remote workflow
revision passed.

The wrapper registers four external integration-test targets, which must be
present and all their tests must be discovered and run. Zero-test unit targets
are expected. Run the feature-isolation check and locked wrapper commands from
the `rs-ioc` checkout:

```sh
python3 .infra/tools/check-manual-feature-isolation.py --manifest-path tests/fixtures/application_consumer_current/Cargo.toml --package ioc-downstream-consumer-current
cargo +1.94.0 check --manifest-path tests/fixtures/application_consumer_current/Cargo.toml --locked
cargo +1.94.0 test --manifest-path tests/fixtures/application_consumer_current/Cargo.toml --locked
cargo +1.94.0 run --manifest-path tests/fixtures/application_consumer_current/Cargo.toml --locked
```

# Current downstream consumer

This wrapper compiles the current sibling `rs-execution-services` consumer's
library, binary, and five public integration-test files. It uses the explicit
library name `ioc_application_consumer`; consumer source is not copied into this
historical fixture. Both consumers disable `qubit-ioc` default features, and the
current lane checks that the resolved direct feature set remains empty.

Prepare these sibling source checkouts at the listed revisions before running
the fixture. The relative paths in the overlay manifest resolve from
`tests/fixtures/application_consumer_current/`.

| Repository | Revision | Package |
| --- | --- | --- |
| `rs-ioc` | current checkout | `qubit-ioc` |
| `rs-execution-services` | `e822fa63080c724aaf1ceb21cb73a0f271d5f86b` | `qubit-execution-services` |
| `rs-fs` | `4cb4ac38f993efb99d52b16ead8aef5f72cfef49` | `qubit-fs` |
| `rs-fs-local` | `3bf4275ed601e39fdbf8532f9d80d563d9cc7182` | `qubit-fs-local` |
| `rs-fs-registry` | `983a8c645b31af2688ea06a55c83d1535852d401` | `qubit-fs-registry` |
| `rs-executor` | `1c543c4b2fd08d8e9fd04d8be7499c122c01be37` | `qubit-executor` |
| `rs-rayon-executor` | `c56ff60318b802a8255efe9e09da63f8f96bbf7c` | `qubit-rayon-executor` |
| `rs-tokio-executor` | `44b010e714cdab03bfc78a85ae7d7309a3faf6c5` | `qubit-tokio-executor` |

The wrapper reads current consumer source from the sibling
`rs-execution-services` checkout. CI source preparation must place these
checkouts side by side with `rs-ioc` and verify each revision before running
the current fixture. Its lockfile is [`Cargo.lock`](Cargo.lock); the native
consumer has a separate lockfile in its own checkout, used by the workflow's
native locked test command. A local checkout or successful local run does not
establish that a remote workflow revision passed. Keep the wrapper lockfile
synchronized when a sibling source revision or registry version changes.

The wrapper registers five external integration-test targets, including the
mixed execution-domain rollback suite with six regression tests. Every target
must be present and all its tests must be discovered and run. Zero-test unit
targets are expected. Run the feature-isolation check and locked wrapper
commands from the `rs-ioc` checkout after preparing the sibling revisions above:

```sh
python3 .infra/tools/check-manual-feature-isolation.py --manifest-path tests/fixtures/application_consumer_current/Cargo.toml --package ioc-downstream-consumer-current
cargo +1.94.0 check --manifest-path tests/fixtures/application_consumer_current/Cargo.toml --locked
cargo +1.94.0 test --manifest-path tests/fixtures/application_consumer_current/Cargo.toml --locked
cargo +1.94.0 run --manifest-path tests/fixtures/application_consumer_current/Cargo.toml --locked
```

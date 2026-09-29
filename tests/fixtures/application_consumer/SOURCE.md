# Historical downstream consumer

This fixture preserves the consumer from `qubit-ltd/rs-execution-services` commit
`c04d5d112dbf0717d44bfd9786109f84ae3cc214`, path
`tests/fixtures/ioc_application_consumer/src/main.rs`.
The original file SHA256 is `0655141ca927a5fe780fafba4a9f0bb0583a8205581993ce87bee5a3c8eb19d0`.
The source is split into this fixture's `src/lib.rs`, `src/main.rs`, and
`tests/application_lifecycle_tests.rs`; it never follows the current consumer.
Both wrappers explicitly name their library `ioc_application_consumer`.

| Sibling checkout | Historical commit | Role |
| --- | --- | --- |
| rs-execution-services | c04d5d112dbf0717d44bfd9786109f84ae3cc214 | Original consumer and execution services 0.10.0 |
| rs-event-bus | f19afc39f82b402b9793a34b8b0f0f23d2364dfe | Historical EventBus 0.15.0 |
| rs-config | 68a331501cdd7911964dd2399e6fc0aee39f8527 | Recorded checkout; IoC resolves locked crates.io qubit-config 0.14.3 |
| rs-fs-registry | d3a6cacbc05bea970175db9cf6211680aa39c87c | Historical registry 0.7.2 |
| rs-ioc | Revision under test | Current Application/BuildFailure API |

The original lane declared/locked EventBus 0.17.0 despite pinning a repository
whose package is 0.15.0. This fixture corrects its constraint to 0.15 and changes
only that package version in the historical lock; all other locked registry
versions/checksums are retained. It does not upgrade the historical checkout.

## Ownership adaptation and coverage

The application owns ExecutionServices abort (`stop`) and asynchronous termination
wait. It explicitly selects `WaitPolicy::unbounded()`. Build failures are inspected
through `BuildFailure::cause()`; tests take and drive rollback ownership explicitly.
Cancelling an async build requests abort once without starting a wait.

The historical EventBus lacks request/ticket shutdown. It is an ordinary external
component, with its caller retaining shutdown responsibility. Main explicitly
calls the old synchronous `Graceful { timeout: 5 seconds }` shutdown, outside all
Managed callbacks, before requesting Immediate application shutdown. No blocking
old EventBus shutdown is installed in Managed abort.

The four original regressions cover missing registry without factory execution,
async factory failure cleanup, cancellation abort exactly once, and synchronous
factory failure cleanup. Main verifies singleton/source lookup, empty registry,
real IO result 43, and termination. This lane does not cover managed EventBus
request/ticket, new FlushWorker final publish, rooted local filesystem integration,
or graceful deadline/cancel/resume tests; those belong to the current lane.

## Reproduction

Check out the table's repositories beside rs-ioc, then run from rs-ioc:

```sh
cargo +1.94.0 check --manifest-path tests/fixtures/application_consumer/Cargo.toml --locked
cargo +1.94.0 test --manifest-path tests/fixtures/application_consumer/Cargo.toml --locked
cargo +1.94.0 run --manifest-path tests/fixtures/application_consumer/Cargo.toml --locked
```

CI logs exact checkout revisions, rustc/cargo versions, rustc executable SHA256,
original consumer source SHA256, adapted source SHA256, and lock SHA256. Focused
local verification uses independent `/tmp` clones with the exact historical SHAs;
empty workspace boundaries in those temporary dependency manifests avoid capture
by an unrelated ancestor workspace and do not alter production source.

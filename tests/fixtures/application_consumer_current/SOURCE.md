# Current downstream consumer

This wrapper compiles the current sibling rs-execution-services consumer's
`src/lib.rs`, `src/main.rs`, and all four public integration-test files. The
explicit library name is `ioc_application_consumer`, matching the real consumer.
No current consumer source is copied into the historical fixture.
The wrapper and the real consumer both declare `qubit-ioc` with
`default-features = false`. The current CI lane checks the resolved direct
`qubit-ioc` feature set with `check-manual-feature-isolation.py`; it must be
empty, including after Cargo feature unification.

| Sibling checkout | Source selection |
| --- | --- |
| rs-ioc | Revision under test, local path qubit-ioc and its macros |
| rs-execution-services | Current-lane checkout ref is defined in [`downstream-contracts.yml`](../../../.github/workflows/downstream-contracts.yml) |
| rs-event-bus | Current-lane checkout ref is defined in [`downstream-contracts.yml`](../../../.github/workflows/downstream-contracts.yml); the historical lane has its own pin |
| rs-config | Current-lane checkout ref is defined in [`downstream-contracts.yml`](../../../.github/workflows/downstream-contracts.yml); IoC uses locked registry qubit-config |
| rs-fs-registry | Current-lane checkout ref is defined in [`downstream-contracts.yml`](../../../.github/workflows/downstream-contracts.yml); direct path and crates.io patch |
| rs-fs-local | Current-lane checkout ref is defined in [`downstream-contracts.yml`](../../../.github/workflows/downstream-contracts.yml); path with registry feature |
| rs-fs | Current-lane checkout ref is defined in [`downstream-contracts.yml`](../../../.github/workflows/downstream-contracts.yml); direct path and crates.io patch |

All sibling checkouts share one parent directory. Path/patch routing keeps Fs and
FsRegistry identities uniform even for transitive fs-local dependencies. The
current lane builds the IoC revision under test with workflow-selected external
checkouts. The workflow records exact source revisions; a local path checkout
alone is not evidence of a passing remote CI run.

The overlay commands use `rs-ioc/tests/fixtures/application_consumer_current/Cargo.lock`.
The current CI lane also runs `cargo +1.94.0 test --manifest-path
tests/fixtures/ioc_application_consumer/Cargo.toml --locked` from the
`rs-execution-services` checkout. That command uses the downstream fixture's
own `Cargo.lock`; updating the downstream checkout ref alone would not test
that lockfile through the wrapper commands.

The current consumer source also observes `BuildFailure` cleanup before returning
its cause, uses Graceful shutdown after successful business work, and waits for
Immediate shutdown before returning a business error. Its successful run checks
the actual `spawn_io` results 43 and 22 and handler message `final-report:22`.
The rooted provider resolves `file:///report.csv` with `stat` length 22.
The current direct dependency constraints are defined by this overlay's
[`Cargo.toml`](Cargo.toml) and the native consumer manifest at
`rs-execution-services/tests/fixtures/ioc_application_consumer/Cargo.toml`.
Fs and FsRegistry use matching direct paths and crates.io patches. The current
lane's checkout refs come from [`downstream-contracts.yml`](../../../.github/workflows/downstream-contracts.yml);
the historical lane has its own pin and manifest.

The wrapper registers application_lifecycle_tests (5 tests),
managed_event_bus_tests (2), resource_integration_tests (2), and
shutdown_policy_tests (4): 13 external tests in total. Coverage includes real
final typed-message flush before dependencies stop, rooted provider metadata,
nonblocking EventBus requests during a gated real handler, abort upgrade,
cancellation/resume, explicit rollback, and injected graceful/termination
budgets. Unit-test targets containing zero tests are expected; the four external
targets must be present and all 13 tests discovered.

```sh
python3 .infra/tools/check-manual-feature-isolation.py --manifest-path tests/fixtures/application_consumer_current/Cargo.toml --package ioc-downstream-consumer-current
cargo +1.94.0 check --manifest-path tests/fixtures/application_consumer_current/Cargo.toml --locked
cargo +1.94.0 test --manifest-path tests/fixtures/application_consumer_current/Cargo.toml --locked
cargo +1.94.0 run --manifest-path tests/fixtures/application_consumer_current/Cargo.toml --locked
```

The lockfile is validated against the exact sibling revisions in
`downstream-contracts.yml`. A different local sibling checkout may require
lockfile changes and does not establish whether the pinned CI lane passes.

CI logs exact checkout revisions, rustc/cargo versions, rustc executable SHA256,
consumer sources and lock SHA256. Local working-tree verification records its
current source, which may differ from a remote checkout until the corresponding
commit is pushed. The historical lane has its own EventBus 0.20 manifest and
lockfile, but retains its earlier consumer source; it does not exercise the
current request/ticket adapter. Both lanes must run against their workflow refs
to establish remote CI coverage.

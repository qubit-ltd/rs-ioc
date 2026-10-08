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
| rs-execution-services | Current-lane CI checkout ref is defined in `downstream-contracts.yml` |
| rs-event-bus | Both CI lanes pin 563552b58fe5cfa93f0f4f4295e7ca4e70911be1, EventBus 0.20.0 |
| rs-config | 80d80b0d3c282bcc441b2c1cff1539941f501594 checkout recorded; IoC uses locked registry qubit-config |
| rs-fs-registry | 8e8940cc295b0e277241c397bea559214a272fb5, direct path and crates.io patch |
| rs-fs-local | 7f0959f15355d3be97c681f588f0ab23ce8d40d1, path with registry feature |
| rs-fs | 4bc4d9e430719d56429f65224a112d640af51478, direct path and crates.io patch |

All sibling checkouts share one parent directory. Path/patch routing keeps Fs and
FsRegistry identities uniform even for transitive fs-local dependencies. The
current lane builds the IoC revision under test with workflow-selected external
checkouts. The workflow records exact source revisions; a local path checkout
alone is not evidence of a passing remote CI run.

The current consumer source also observes `BuildFailure` cleanup before returning
its cause, uses Graceful shutdown after successful business work, and waits for
Immediate shutdown before returning a business error. Its successful run checks
the actual `spawn_io` results 43 and 22 and handler message `final-report:22`.
The rooted provider resolves `file:///report.csv` with `stat` length 22.
Current path dependencies require qubit-ioc 0.3.0 and qubit-event-bus 0.20.0;
Fs and FsRegistry use matching direct paths and crates.io patches.

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

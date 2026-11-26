# Historical downstream consumer

This wrapper preserves the historical consumer source from
`qubit-ltd/rs-execution-services` commit
`c04d5d112dbf0717d44bfd9786109f84ae3cc214`, originally at
`tests/fixtures/ioc_application_consumer/src/main.rs` (SHA256
`0655141ca927a5fe780fafba4a9f0bb0583a8205581993ce87bee5a3c8eb19d0`). The
fixture keeps its split library, binary, and lifecycle-test sources in this
repository; it does not follow the current consumer fixture. Its IoC library
name is `ioc_application_consumer`.

## Historical source inputs

For local historical verification, check out these revisions as siblings of
`rs-ioc` under one `rust-common` directory. The relative paths in the fixture
manifest resolve from that layout.

| Checkout | Revision | Role |
| --- | --- | --- |
| `rs-execution-services` | `c04d5d112dbf0717d44bfd9786109f84ae3cc214` | Consumer origin and `qubit-execution-services` 0.10.0 |
| `rs-event-bus` | `f19afc39f82b402b9793a34b8b0f0f23d2364dfe` | Historical `qubit-event-bus` 0.15.0 |
| `rs-config` | `98efea94b2d5b83c9f26f230d75e6cde202e1931` | `qubit-config` 0.15 required by current IoC |
| `rs-fs-registry` | `d3a6cacbc05bea970175db9cf6211680aa39c87c` | Historical registry 0.7.2 |

The fixture manifest selects these path packages and the lockfile records their
resolved identities. The historical CI job in
[`downstream-contracts.yml`](../../../.github/workflows/downstream-contracts.yml)
checks out these exact revisions beside the current `rs-ioc` candidate and
runs the locked metadata, check, test, and binary commands. Developers use the
same sibling layout and keep local target output separate. CI owns the remote
reproduction; a local run does not imply that CI passed.

## Baseline and verification evidence

The original pre-T6 dependency combination was checked against these exact
historical siblings before adaptation. Locked metadata exited 101 because the
manifest required execution-services `^0.11`, while the pinned historical
checkout provides 0.10.0:

```text
cargo +1.94.0 metadata --locked --all-features --format-version 1 --manifest-path tests/fixtures/application_consumer/Cargo.toml
error: failed to select a version for the requirement `qubit-execution-services = "^0.11"`
candidate versions found which didn't match: 0.10.0
```

The wrapper was adapted to the exact historical package versions: execution
services 0.10.0, EventBus 0.15.0, and registry 0.7.2. It patches `qubit-config`
to historical source 0.15.0 because the current IoC default config feature
requires that package. No consumer source was changed for this dependency
adaptation.

At IoC candidate `9d7889dfc1fa5b4e3ee22eca8e1610597dbd3d9f`, locked metadata
resolved these path package IDs (the stable manifest-relative forms are shown;
Cargo prints absolute machine-specific paths):

| Package ID | Source checkout SHA |
| --- | --- |
| `qubit-ioc@0.4.3` at `rs-ioc` | `9d7889dfc1fa5b4e3ee22eca8e1610597dbd3d9f` |
| `qubit-execution-services@0.10.0` at `rs-execution-services` | `c04d5d112dbf0717d44bfd9786109f84ae3cc214` |
| `qubit-event-bus@0.15.0` at `rs-event-bus` | `f19afc39f82b402b9793a34b8b0f0f23d2364dfe` |
| `qubit-config@0.15.0` at `rs-config` | `98efea94b2d5b83c9f26f230d75e6cde202e1931` |
| `qubit-fs-registry@0.7.2` at `rs-fs-registry` | `d3a6cacbc05bea970175db9cf6211680aa39c87c` |

The refreshed `Cargo.lock` SHA256 is
`3a0c165141c1009511a1cf0b505095d832ce4183574cb0ea51abc1a54812ac99`. The
following commands each exited 0 with Rust/Cargo 1.94.0 and an independent
target directory; the test command ran all four historical lifecycle tests:

```text
cargo +1.94.0 metadata --locked --all-features --format-version 1 --manifest-path tests/fixtures/application_consumer/Cargo.toml — exit 0
cargo +1.94.0 check --locked --all-targets --manifest-path tests/fixtures/application_consumer/Cargo.toml — exit 0
cargo +1.94.0 test --locked --manifest-path tests/fixtures/application_consumer/Cargo.toml — exit 0 (4 passed)
cargo +1.94.0 run --locked --manifest-path tests/fixtures/application_consumer/Cargo.toml — exit 0
```

## Reproduction

Run from the `rs-ioc` checkout after preparing the sibling revisions above:

```sh
cargo +1.94.0 metadata --locked --all-features --format-version 1 --manifest-path tests/fixtures/application_consumer/Cargo.toml
cargo +1.94.0 check --locked --all-targets --manifest-path tests/fixtures/application_consumer/Cargo.toml
cargo +1.94.0 test --locked --manifest-path tests/fixtures/application_consumer/Cargo.toml
cargo +1.94.0 run --locked --manifest-path tests/fixtures/application_consumer/Cargo.toml
```

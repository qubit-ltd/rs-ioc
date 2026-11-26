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

The pre-T6 wrapper baseline in the supplied T6 worktree failed locked metadata
with exit 101 because Cargo needed to update its stale lockfile:

```text
cargo +1.94.0 metadata --locked --all-features --format-version 1 --manifest-path tests/fixtures/application_consumer/Cargo.toml
error: cannot update the lock file because --locked was passed to prevent this
```

This records the observed baseline failure; it does not establish API
incompatibility. The historical dependency versions above are retained. The
exact historical checkout combination still needs its own metadata, check,
test, and run results before the historical lane is considered verified.

## Reproduction

Run from the `rs-ioc` checkout after preparing the sibling revisions above:

```sh
cargo +1.94.0 metadata --locked --all-features --format-version 1 --manifest-path tests/fixtures/application_consumer/Cargo.toml
cargo +1.94.0 check --locked --all-targets --manifest-path tests/fixtures/application_consumer/Cargo.toml
cargo +1.94.0 test --locked --manifest-path tests/fixtures/application_consumer/Cargo.toml
cargo +1.94.0 run --locked --manifest-path tests/fixtures/application_consumer/Cargo.toml
```

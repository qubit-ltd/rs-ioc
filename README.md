# qubit-ioc

[![Rust CI](https://github.com/qubit-ltd/rs-ioc/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-ioc/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-ioc/coverage-badge.json)](https://qubit-ltd.github.io/rs-ioc/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-ioc.svg?color=blue)](https://crates.io/crates/qubit-ioc)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![中文文档](https://img.shields.io/badge/文档-中文版-blue.svg)](README.zh_CN.md)

`qubit-ioc` helps Rust application authors assemble shared components across
crates at startup. It validates dependencies before running factories, so a
missing or ambiguous service fails during startup instead of a later request.
Successful construction returns an `Application` lifecycle owner with a cloneable,
read-only `ApplicationContext`; applications can use attribute macros or explicit
registration.

## Installation

Add `qubit-ioc` to your dependencies:

```toml
[dependencies]
qubit-ioc = "0.3"
```

The default features are `macros` and `config`. To use only the manual runtime,
set `default-features = false` on the application's `qubit-ioc` dependency.
Cargo unifies features across dependency paths, so another dependency that
enables the defaults can still activate them.

| Feature | Default | Purpose |
| --- | --- | --- |
| `macros` | Yes | `#[Component]`, `#[Service]`, `#[Repository]`, `#[Configuration]`, `#[ConfigurationProperties]`, and `#[bean]`. |
| `config` | Yes | Register a `qubit-config` snapshot and use `#[value]` or `#[ConfigurationProperties]`. |

The `macros` and `config` features are independent. Configuration attributes
such as `#[value]` and `#[ConfigurationProperties]` require both features;
enabling `config` alone does not enable the macros.

## Quick start: assemble a greeting service

Suppose a service needs a greeting implementation from another crate. Declare
the concrete component and bind it to the trait, then select the service as a
build root. The macro cannot infer the trait binding from a separate `impl`
block, so `bind = dyn Greeting` is required.

```rust
use std::sync::Arc;
use qubit_ioc::{Application, Component, Service};

trait Greeting: Send + Sync {
    fn text(&self) -> &'static str;
}

#[Component(bind = dyn Greeting, id = "example.greeting.english", primary)]
struct English;

impl Greeting for English {
    fn text(&self) -> &'static str { "hello" }
}

#[Service]
struct Greeter {
    greeting: Arc<dyn Greeting>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = Application::builder();
    builder.install::<English>()?;
    builder.install::<Greeter>()?;
    builder.root::<Greeter>();
    let application = builder.build()?;
    let context = application.context();
    assert_eq!(context.get::<Greeter>()?.greeting.text(), "hello");
    Ok(())
}
```

Run `cargo run --example readme_declarative` to execute this example; its full
source is in [`examples/readme_declarative.rs`](examples/readme_declarative.rs).
`build()` requires at least one selected root. Use `build_all()` to construct
all active definitions.

For an application assembly check, select roots and set
`validation_scope(ValidationScope::AllActive)` to validate every active
definition before publishing the selected graph. Unselected factories do not
run, and an unselected async or managed factory does not require async build
or a `WaitPolicy`. Keep the default `Reachable` scope when a profile intentionally
uses only one root closure. `build_all()` constructs every active definition;
it is a different choice from `AllActive` validation of selected roots.

The assertion observes the selected implementation. The builder checks the
selected service's dependency graph before constructing either component.

## What it provides

- **Explicit assembly and selection:** Register instances and factories manually or declare components with macros. Select dependencies by type or ID; `Option<Arc<T>>` and `Vec<Arc<T>>` express optional and collection requests. See [Choosing definitions and build scope](doc/user_guide.md#choosing-definitions-and-build-scope).
- **Configuration and function beans:** `#[bean]` supports synchronous, asynchronous, and managed factories, and `#[Configuration]` groups registrations. See the [function bean scenario](doc/user_guide.md#scenario-function-beans-and-configuration-groups).
- **Async construction:** Selected async factories require `build_async()` or `build_all_async()`. Manual typed factory registration is explained in [Typed factories](doc/user_guide.md#typed-factories-and-application-shutdown-budgets).
- **Application shutdown:** `Managed<T>` describes stop and wait actions. Retain the `Application` owner and explicitly await `ShutdownHandle::wait()` to observe termination; deadlines cannot interrupt blocking work. See the [lifecycle guide](doc/lifecycle.md) and [managed adapter guide](doc/managed-adapters.md).

## Build failures and resource boundaries

`build()` and `build_async()` return `BuildFailure` after requesting abort; the caller can keep it to observe cleanup. The four settled build methods await rollback observation during normal completion and preserve the original cause. If startup may be cancelled and cleanup still needs observation, keep a `BuildSession`. See [Build failures and rollback observation](doc/user_guide.md#build-failures-and-rollback-observation) and [Observe cleanup after cancelling an in-progress build](doc/user_guide.md#observe-cleanup-after-cancelling-an-in-progress-build).

## Manual assembly

Disable default features to use the manual runtime; see the [`readme_manual` example](examples/readme_manual.rs) for a complete runnable registration path:

```toml
qubit-ioc = { version = "0.3", default-features = false }
```

The [user guide](doc/user_guide.md) covers roots, profiles, selection rules, errors, and shutdown responsibilities. An independent downstream assembly example is maintained in the [`rs-execution-services` consumer fixture](https://github.com/qubit-ltd/rs-execution-services/blob/main/tests/fixtures/ioc_application_consumer/README.md). From that checkout, run `cargo run --manifest-path tests/fixtures/ioc_application_consumer/Cargo.toml`. This fixture verifies the cross-crate contract; it is not evidence of production adoption.

## Limitations

The container provides application-wide shared instances. It does not provide prototype or request scopes, hot reload, automatic shutdown for unmanaged components, circular proxies, or dynamic-library discovery. Applications must explicitly await shutdown to confirm termination; dropping an owner or handle requests best-effort abort, and deadlines cannot interrupt blocking work. See the [lifecycle guide](doc/lifecycle.md) for shutdown behavior and reports.

## Learn more

Read the [English user guide](doc/user_guide.md) or [中文用户手册](doc/user_guide.zh_CN.md). For shutdown details, see the [English lifecycle guide](doc/lifecycle.md), [中文生命周期指南](doc/lifecycle.zh_CN.md), [English managed adapter guide](doc/managed-adapters.md), and [中文托管资源适配指南](doc/managed-adapters.zh_CN.md). Run `cargo doc --no-deps --open` in this checkout to browse the public API.

## Testing

```bash
# Run tests with the default feature set
cargo test

# Run tests with all declared features
cargo test --all-features

# Project CI checks
.infra/bin/ci-check.sh

# Check code coverage
.infra/bin/coverage.sh
```

## License

Copyright (c) 2025 - 2026. Haixing Hu. All rights reserved.

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for the
full license text.

## Contributing

Contributions are welcome. Please follow the Rust API guidelines and keep
public API documentation and tests current. Use `.infra/bin/align-ci.sh` to
apply this project's formatting and `.infra/bin/style-check.sh` (or
`.infra/bin/ci-check.sh`) to verify it. The project style tool uses
`nightly-2026-06-05`; plain `cargo fmt --all --check` with the package's Rust
1.94 toolchain is not the project's formatting gate.

## Author

**Haixing Hu** - *Qubit Co. Ltd.*

Repository: [https://github.com/qubit-ltd/rs-ioc](https://github.com/qubit-ltd/rs-ioc)

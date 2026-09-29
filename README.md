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

After the `0.3.0` release is published, add the registry dependency:

```toml
[dependencies]
qubit-ioc = "0.3"
```

This checkout is a `0.3.0` release candidate and is not yet available from
crates.io. To develop against a local checkout, use:

```toml
qubit-ioc = { version = "0.3", path = "../rs-ioc" }
```

The default features are `macros` and `config`. To use only the manual
runtime, add `default-features = false` to either dependency declaration.

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
the complete registered graph.

The assertion observes the selected implementation. The builder checks the
selected service's dependency graph before constructing either component.

### Function beans and configuration groups

A function bean generates a registration marker: `default_value` becomes
`DefaultValueBean`, while `#[bean(marker = CustomFactory)]` selects an explicit
marker. Install the markers, call `grouped::register_ioc(&mut builder)?` for a
`#[Configuration]` module, then select roots and query the built context.
The [function bean example](examples/readme_beans.rs) produces `1`, `2`, and
`"ready"` through assertions and needs only `macros`:

```bash
cargo +1.94.0 run --example readme_beans --no-default-features --features macros --locked
```

Follow the [function bean scenario in the user guide](doc/user_guide.md#scenario-function-beans-and-configuration-groups)
for the complete code, async construction, and managed shutdown.

## What it provides

For exact selection, use `#[inject(id = "...")]` on an `Arc<T>` field or bean
parameter. `Option<Arc<T>>` and `Vec<Arc<T>>` express optional and all-candidate
requests. `#[bean]` supports synchronous and asynchronous free functions,
including factories returning `Managed<T>` or `Result<Managed<T>, E>`;
asynchronous definitions require `build_async()`.
The return may use an imported `Managed`, `qubit_ioc::Managed`, or
`::qubit_ioc::Managed`. Renamed dependencies and raw-keyword crate names are
supported too: `ioc::Managed` / `::ioc::Managed` and `r#type::Managed` /
`::r#type::Managed`. These spellings also work inside `Result<Managed<T>, E>`.
Unrelated paths such as `application::Managed` and type aliases remain ordinary
component types.
The macros apply `cfg` activation to generated dependency, registration, and
factory argument code, so conditionally compiled bean parameters stay aligned
with their generated calls. Write `inject` and `value` helper attributes
directly on component fields or bean parameters; putting them inside `cfg_attr`
produces a focused diagnostic. Cross-definition key collisions are checked during build after
profile filtering, not when `register_instance_with` stages an instance.
Create each managed resource inside its managed factory, after graph
validation. Register an already-running external resource with
`register_instance(Arc<T>)` and keep its shutdown responsibility in the
application; do not capture an already-created `Managed<T>` in a factory.
The application retains its unique lifecycle owner while
`application.context().clone()` provides concurrent read-only query handles.
These clones can remain alive during shutdown; a successful lookup does not
guarantee that a component still accepts work. Collection query order is
precomputed when the context is published, using binding order, ID, and source
location. Normal exit uses
`application.begin_shutdown(ShutdownMode::Graceful)` and explicitly awaits the
returned handle. Graceful requests start when `wait()` is first polled;
`ShutdownMode::Immediate` requests all aborts before `begin_shutdown` returns.
Managed graphs require an explicit `WaitPolicy`; use
`WaitPolicy::bounded` with an application-driven timer. Public
`Definition::builder()` and `register_definition` support custom factories and
trait aliases without relying on macro internals. See the [lifecycle and 0.3 migration guide](doc/lifecycle.md).

Config reads from `#[value]` and `ConfigurationProperties` preserve stored
values without interpolation. Structured deserialization rejects unknown
fields by default. When interpolation is required, call
`Config::get_interpolated` explicitly in a factory.
Factory panics follow Rust's panic behavior and propagate. On failure, both
synchronous and asynchronous construction return `BuildFailure` immediately after requesting
abort for transferred managed resources. Inspect `cause()`, take the cleanup
owner with `take_cleanup()` or `into_parts()`, and explicitly await its `wait()`
to observe termination and cleanup failures.

### Manual assembly

Explicit instances and factories work with `default-features = false`:

```rust
use std::sync::Arc;
use qubit_ioc::{ContainerBuilder, Dependency};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(String::from("hello")))?;
    builder.register_factory::<usize, _>(&[Dependency::of::<String>()], |context| {
        let message = context.get::<String>().expect("declared dependency");
        Ok(Arc::new(message.len()))
    })?;
    builder.root::<usize>();
    let application = builder.build()?;
    let context = application.context();
    assert_eq!(*context.get::<usize>()?, 5);
    Ok(())
}
```

Run `cargo run --example readme_manual --no-default-features` to execute this
manual path. Its source is in [`examples/readme_manual.rs`](examples/readme_manual.rs).

Each binding is identified by its Rust type and optional ID. IDs use
dot-separated ASCII segments such as `example.greeting.english`; each segment
starts with a letter and continues with letters, digits, or underscores. An
unnamed request selects a sole candidate or a unique `primary` binding.

Use explicit registration to assemble an application. See the [user guide](doc/user_guide.md) for roots, profiles,
selection rules, error handling, and shutdown responsibilities.
An independent downstream assembly example is maintained in the
[`rs-execution-services` consumer fixture](https://github.com/qubit-ltd/rs-execution-services/blob/main/tests/fixtures/ioc_application_consumer/README.md).
From the `rs-execution-services` checkout, run it with
`cargo run --manifest-path tests/fixtures/ioc_application_consumer/Cargo.toml`.
This fixture verifies the cross-crate contract and does not claim production adoption.
Its EventBus adapter uses `request_shutdown` and a ticket's `wait_async()`;
the synchronous EventBus `shutdown`, including Immediate mode, still waits.

## Limitations

The container provides application-wide shared instances. It does not provide
prototype or request scopes, hot reload, automatic lifecycle management for
unmanaged components, circular proxies, or dynamic-library discovery. Managed
factories can opt into explicit stop and wait actions through `Managed<T>` and
`Application::begin_shutdown` and `ShutdownHandle::wait`. Struct macros support named-field and unit structs;
other shapes can use manual factories. Runtime reflection is not used to
construct components. `qubit-spi` remains responsible for provider selection
and fallback; its registry or a selected service can be registered as a normal
IoC component. Observed managed shutdown requires an explicit shutdown mode and
`ShutdownHandle::wait()`; dropping the owner, an untransferred `Managed`, or a
shutdown handle requests best-effort abort without waiting. Dropping a query
context does not request shutdown. A deadline cannot kill blocking work, and
`ShutdownReport::incomplete()` means termination was not confirmed. See the [lifecycle guide](doc/lifecycle.md),
the runnable [`app_lifecycle` example](examples/app_lifecycle.rs), and the
[English current design](doc/complete-design.md) for design boundaries.

## Learn more

Follow the [English user guide](doc/user_guide.md) or
[中文用户手册](doc/user_guide.zh_CN.md) for setup, selection, errors, and shutdown.
The [English current design](doc/complete-design.md) and
[中文当前设计](doc/complete-design.zh_CN.md) describe the public contracts.
Run `cargo doc --no-deps --open` in this checkout to browse the public API.

## Testing

```bash
# Run tests with the default feature set
cargo test

# Run tests with all declared features
cargo test --all-features

# Project CI checks
./ci-check.sh

# Check code coverage
./coverage.sh
```

## License

Copyright (c) 2025 - 2026. Haixing Hu. All rights reserved.

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for the
full license text.

## Contributing

Contributions are welcome. Please follow the Rust API guidelines, keep public
API documentation and tests current, and run `./align-ci.sh` to format code and
`./ci-check.sh` to satisfy CI requirements before submitting a pull request.

## Author

**Haixing Hu** - *Qubit Co. Ltd.*

Repository: [https://github.com/qubit-ltd/rs-ioc](https://github.com/qubit-ltd/rs-ioc)

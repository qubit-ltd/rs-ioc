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
add `default-features = false` to the dependency declaration.

| Feature | Default | Purpose |
| --- | --- | --- |
| `macros` | Yes | `#[Component]`, `#[Service]`, `#[Repository]`, `#[Configuration]`, `#[ConfigurationProperties]`, and `#[bean]`. |
| `config` | Yes | Register a `qubit-config` snapshot and use `#[value]` or `#[ConfigurationProperties]`. |

The `macros` and `config` features are independent. Configuration attributes
such as `#[value]` and `#[ConfigurationProperties]` require both features;
enabling `config` alone does not enable the macros.

Manual factories can use `register_injected_factory`,
`register_injected_managed_factory`, `register_injected_async_factory`, and
`register_injected_managed_async_factory` to derive dependency requests from
typed argument tuples. The tuple supports `()`, required `Arc<T>`, optional
`Option<Arc<T>>`, and all-candidate `Vec<Arc<T>>` arguments, with zero through
eight arguments. An async factory requires `build_async()` or
`build_all_async()`. For named IDs or custom dependency requests, use
`register_async_factory` or `register_managed_async_factory` directly.

This complete async registration example derives its `String` dependency from
the argument type:

```rust
use std::error::Error;
use std::sync::Arc;
use qubit_ioc::ContainerBuilder;

async fn build_message_length() -> Result<(), Box<dyn Error>> {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(String::from("hello")))?;
    builder.register_injected_async_factory::<usize, (Arc<String>,), _>(
        |(message,)| Box::pin(async move { Ok(Arc::new(message.len())) }),
    )?;
    let application = builder.build_all_async().await?;
    assert_eq!(*application.context().get::<usize>()?, 5);
    Ok(())
}
```

Managed factories should create resources after graph validation; side effects
that happen before a factory returns `Managed<T>` remain the factory's
responsibility. Managed shutdown can optionally use
`WaitPolicy::bounded_with_total(grace, termination, total, timer)` for one
application-wide budget in addition to per-component budgets. See the
[lifecycle guide](doc/lifecycle.md) for deadline and reporting semantics.



## Choose how build failures observe rollback

The original `build()` and `build_async()` return `BuildFailure` after requesting abort. Use them when immediate return matters, or retain the failure and resume `wait_cleanup()` after cancellation. Settled methods await rollback during normal completion and return `SettledBuildFailure`, which keeps the original `BuildError` as its first error source and exposes the optional final `ShutdownReport` through `cleanup_report()`. Cleanup can still fail or remain incomplete.

```rust
use std::error::Error;
use std::sync::Arc;
use qubit_ioc::{
    CleanupError, ContainerBuilder, Dependency, FactoryError, Managed,
    SettledBuildFailure, WaitPolicy,
};

struct Worker;
struct Startup;

async fn fail_after_worker(
    fail_cleanup: bool,
) -> Result<(SettledBuildFailure, bool), Box<dyn Error>> {
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder.register_managed_factory::<Worker, _>(&[], move |_| {
        Ok(Managed::synchronous(Arc::new(Worker), move |_| {
            if fail_cleanup {
                Err(CleanupError::new(std::io::Error::other("cleanup failed")))
            } else {
                Ok(())
            }
        }))
    })?;
    builder.register_factory::<Startup, _>(&[Dependency::of::<Worker>()], |_| {
        Err(FactoryError::new(std::io::Error::other("startup failed")))
    })?;
    builder.root::<Startup>();

    let failure = match builder.build_settled().await {
        Ok(_) => unreachable!("the Startup factory always fails"),
        Err(failure) => failure,
    };
    let cleanup_succeeded = failure
        .cleanup_report()
        .expect("Worker was constructed before Startup failed")
        .is_success();
    Ok((failure, cleanup_succeeded))
}

async fn show_cleanup_reports() -> Result<(), Box<dyn Error>> {
    let (failure, cleanup_succeeded) = fail_after_worker(false).await?;
    assert!(cleanup_succeeded);
    println!("build cause: {}", failure.cause());

    let (failure, cleanup_succeeded) = fail_after_worker(true).await?;
    assert!(!cleanup_succeeded);
    eprintln!("build cause: {}; cleanup report: {:?}", failure.cause(), failure.cleanup_report());
    // An application can return Err(Box::new(failure)) after inspecting both.
    Ok(())
}
```

In this example, `Startup` fails after `Worker` was constructed, so `cleanup_report()` is present. Calling `show_cleanup_reports()` reaches both a successful cleanup report and a failed cleanup report. A graph/preflight failure instead has no managed cleanup and therefore no report. The four settled entry points are `build_settled()`, `build_all_settled()`, `build_async_settled()`, and `build_all_async_settled()`. Cancelling/dropping a settled future can interrupt its wait: cancellation only requests best-effort abort and does not guarantee rollback observation completes.

For one whole-shutdown budget, explicitly select `WaitPolicy::bounded_with_total(grace, termination, total, timer)`. Its total timer starts when `ShutdownHandle::wait()` is first polled, persists across cancellation, and cannot preempt synchronous blocking callbacks or a blocking future poll.

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
guarantee that a component still accepts work. `ApplicationContext::state()`
reports lifecycle state but does not block queries after shutdown. Collection query order is
precomputed when the context is published, using binding order, ID, and source
location. Normal exit uses
`application.begin_shutdown(ShutdownMode::Graceful)` and explicitly awaits the
returned handle. Graceful requests start when `wait()` is first polled;
`ShutdownMode::Immediate` requests all aborts before `begin_shutdown` returns.
Managed graphs require an explicit `WaitPolicy`; use
`WaitPolicy::bounded` with an application-driven timer. Public
`Definition::builder()` and `register_definition` support custom factories and
trait aliases without relying on macro internals. Choose
`Managed::synchronous` when successful stop confirms termination, or
`Managed::asynchronous` to pair a non-blocking stop request with a termination
wait. Choose graceful behavior at construction with
`Managed::synchronous_with_graceful` or
`Managed::asynchronous_with_graceful`; a ticket-producing request uses its
ticket-aware constructor to keep the request and wait callbacks paired. When a
request returns a shutdown ticket, use
`Managed::asynchronous_with_ticket` or
`Managed::asynchronous_with_graceful_ticket`; the latter accepts separate
Immediate and Graceful requests. Graceful callbacks are fixed by the selected
constructor and cannot be replaced afterward. The ticket constructors preserve
the ticket produced by their graceful request for the matching wait callback.
For example, the following API path compiles
without an external runtime:

```rust
use std::sync::Arc;
use qubit_ioc::{BuildError, BuildFailure, CleanupError, Managed};

async fn ticket_and_failure() {
    let _managed = Managed::asynchronous_with_graceful_ticket(
        Arc::new(()),
        |_| Ok::<u64, CleanupError>(2),
        |_| Ok::<u64, CleanupError>(1),
        |_, ticket| Box::pin(async move {
            assert!(ticket == 1 || ticket == 2);
            Ok(())
        }),
    );
    let mut failure = BuildFailure::from(BuildError::NoRootsSelected);
    assert!(failure.wait_cleanup().await.is_none());
}
```

The ticket destructor must not cancel resource shutdown. See the
[managed adapter guide](doc/managed-adapters.md) and
[lifecycle and 0.3 migration guide](doc/lifecycle.md).

Config reads from `#[value]` and `ConfigurationProperties` preserve stored
values without interpolation. Structured deserialization rejects unknown
fields by default. When interpolation is required, call
`Config::get_interpolated` explicitly in a factory.
Factory panics follow Rust's panic behavior and propagate. On failure, the original synchronous and asynchronous build methods return
`BuildFailure` immediately after requesting abort for transferred managed
resources. For normal-completion rollback observation, use one of the settled
build methods described above. Call
`failure.wait_cleanup().await` to observe the optional `ShutdownReport` while
keeping the original `BuildError` available through `cause()`. If that wait is
cancelled, call it again on the same failure to resume cleanup observation;
cleanup errors remain in the report and do not replace the build error.
`take_cleanup()` and `into_parts()` remain available when the caller needs to
own the cleanup handle directly.

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
The [managed adapter guide](doc/managed-adapters.md) and
[中文托管适配指南](doc/managed-adapters.zh_CN.md) explain stop and wait contracts.
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
.infra/bin/ci-check.sh

# Check code coverage
.infra/bin/coverage.sh
```

## License

Copyright (c) 2025 - 2026. Haixing Hu. All rights reserved.

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for the
full license text.

## Contributing

Contributions are welcome. Please follow the Rust API guidelines, keep public
API documentation and tests current, and run `.infra/bin/align-ci.sh` to format code and
`.infra/bin/ci-check.sh` to satisfy CI requirements before submitting a pull request.

## Author

**Haixing Hu** - *Qubit Co. Ltd.*

Repository: [https://github.com/qubit-ltd/rs-ioc](https://github.com/qubit-ltd/rs-ioc)

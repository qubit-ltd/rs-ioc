# qubit-ioc User Guide

[中文用户手册](user_guide.zh_CN.md) · [README](../README.md)

This guide is for Rust application authors using the `qubit-ioc` 0.2.0 release candidate
checkout. It explains how to assemble application-wide shared components,
diagnose startup failures, and manage their lifetime. Rust 1.94 or newer is
required by the package manifest.

## Conceptual model

| Term | Meaning |
| --- | --- |
| Definition | A staged instance or factory; registration does not run a factory. |
| Binding | A Rust type and optional, case-sensitive ID that identify a component. |
| Dependency | A request declared by a factory or generated from a macro field. |
| Root | A requested component whose dependency closure `build()` constructs. |
| Context | The read-only `ApplicationContext` published after a successful build. |

The builder filters active profiles, resolves roots and dependencies, validates
the graph, and then runs factories in dependency order. `build_all()` constructs
all active definitions instead of selecting a root closure. Neither method
publishes a partial context after failure.

## Scenario: start a service with a shared setting

An application has a greeting text and a service that needs it. The goal is to
build the service at startup, observe its output, and reuse the same service
instance. This path uses explicit registration, so it also works with all
default features disabled.

### Installation and configuration

In an application beside this checkout, add:

```toml
[dependencies]
qubit-ioc = { version = "0.2", path = "../rs-ioc", default-features = false }
```

Place the following code in `src/main.rs`, then run `cargo run` in that
application:

```rust
use std::sync::Arc;
use qubit_ioc::{ContainerBuilder, Dependency};

struct Greeter {
    message: Arc<String>,
}

impl Greeter {
    fn greet(&self) -> &str {
        self.message.as_str()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(String::from("hello")))?;
    builder.register_factory::<Greeter, _>(&[Dependency::of::<String>()], |context| {
        let message = context.get::<String>().expect("declared dependency");
        Ok(Arc::new(Greeter { message }))
    })?;
    builder.root::<Greeter>();

    let context = builder.build()?;
    let first = context.get::<Greeter>()?;
    let second = context.get::<Greeter>()?;
    assert_eq!(first.greet(), "hello");
    assert!(Arc::ptr_eq(&first, &second));
    println!("{}", first.greet());
    Ok(())
}
```

The command prints `hello`. `Dependency::of::<String>()` declares what the
factory may read; `root::<Greeter>()` selects the service and its setting for
construction. The two queries clone handles to the same stored component.
For a repository example with explicit startup and shutdown calls, run
`cargo run --example app_lifecycle` from this repository.

## Scenario: function beans and configuration groups

When a provider constructs values with free functions, use `#[bean]` to turn
those functions into installable definitions. This runnable introductory
example verifies a default marker, a custom marker, and a module containing
related beans. It uses Rust 2024, Rust 1.94, and only the `macros` feature:

```toml
qubit-ioc = { version = "0.2", path = "../rs-ioc", default-features = false, features = ["macros"] }
```

```rust
use std::sync::Arc;

use qubit_ioc::ContainerBuilder;
use qubit_ioc::bean;

struct DefaultValue(u8);
struct CustomValue(u8);

#[bean]
fn default_value() -> DefaultValue {
    DefaultValue(1)
}

#[bean(marker = CustomFactory)]
fn custom_value() -> Arc<CustomValue> {
    Arc::new(CustomValue(2))
}

#[qubit_ioc::Configuration]
mod grouped {
    #[qubit_ioc::bean]
    fn label() -> String {
        "ready".to_owned()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = ContainerBuilder::new();
    builder.install::<DefaultValueBean>()?;
    builder.install::<CustomFactory>()?;
    grouped::register_ioc(&mut builder)?;
    builder.root::<DefaultValue>();
    builder.root::<CustomValue>();
    builder.root::<String>();
    let context = builder.build()?;
    assert_eq!(context.get::<DefaultValue>()?.0, 1);
    assert_eq!(context.get::<CustomValue>()?.0, 2);
    assert_eq!(context.get::<String>()?.as_str(), "ready");
    Ok(())
}
```

A function's default marker is its PascalCase name followed by `Bean`:
`default_value` generates `DefaultValueBean`. `marker = CustomFactory`
overrides that name. Returning `Arc<CustomValue>` stores the inner component
as `CustomValue`, so roots and queries use `CustomValue`. A
`#[Configuration]` module exports `register_ioc`; calling it installs the
module's beans without running them. This grouping attribute needs no
`config` feature. Reading configuration with `#[value]` or
`#[ConfigurationProperties]` needs both `macros` and `config`.

The three roots select the definitions; the assertions observe `1`, `2`, and
`"ready"` after construction. Run the identical
[repository example](../examples/readme_beans.rs) from this checkout:

```bash
cargo +1.94.0 run --example readme_beans --no-default-features --features macros --locked
```

In a provider crate, export a registration entry point and let the consuming
application call it during startup. Add `async` to a bean only when its
construction needs to await work; drive `builder.build_async().await?` with
the application's executor. A synchronous build rejects a selected async
factory with `BuildError::AsyncRequired` before any factory or alias projector
runs. A graph-validation error likewise prevents construction. A later
factory failure can still roll back resources from earlier managed factories.

For a bean that starts a worker, create the worker inside the factory and
return `Managed<T>` with a stop action and, when needed, `.with_wait(...)`.
At application exit, release shared context handles, obtain its single owner,
then use this fragment inside an async function:

```rust,ignore
let mut shutdown = context.begin_shutdown();
shutdown.wait().await?;
```

`begin_shutdown()` sends every stop request before returning; `wait()` observes
termination and cleanup errors. `Managed` and `ShutdownHandle` are `must_use`:
return managed values to the container and observe the shutdown handle. To
request stop while deliberately abandoning waits, explicitly write
`drop(context.begin_shutdown())`. Dropping the context alone does not stop
workers. Cancellation only requests stop for resources already returned as
`Managed`; effects from an unfinished factory stay that factory's
responsibility. Follow the [lifecycle guide](lifecycle.md) and
[managed worker example](../examples/app_lifecycle.rs) for a full implementation.

## Choosing definitions and build scope

With default features, `#[Component]`, `#[Service]`, and `#[Repository]`
generate definitions. `#[bean]` generates a factory definition. Install each
definition with `builder.install::<T>()?`, or call a provider crate's
`register_ioc(&mut builder)` assembly function. Declare a trait alias with
`bind = dyn Trait`; a separate `impl` block alone does not create that binding.

The `macros` and `config` features can be enabled independently. Component and
bean declarations need `macros`. `#[value]` and `#[ConfigurationProperties]`
need both `macros` and `config`; enabling `config` alone does not export those
macros. Field and bean-parameter `cfg` conditions, including nested
`cfg_attr(..., cfg(...))`, also control generated dependency requests,
initialization, and factory call arguments. Put `inject` and
`value` helpers directly on component fields or bean parameters; nested helpers inside `cfg_attr` are
rejected with a diagnostic. With `default-features = false`, manual
registration needs neither feature.

Call `root::<T>()` for an unnamed request or `root_by_id::<T>("some.id")?` for
an exact binding. `build()` needs at least one root and constructs only its
dependency closure. `build_all()` constructs every active definition and can
build an empty graph. Both have asynchronous counterparts. A graph with an
asynchronous factory requires `build_async()` or `build_all_async()` and an
executor supplied by the application.

If several bindings have the same Rust type, an unnamed request selects the
sole candidate or the sole `primary` candidate. Otherwise it is ambiguous.
Use a valid ID for exact selection; each dot-separated ASCII segment must
start with a letter and contain only letters, digits, or underscores.
`get_by_id::<T>()`, `try_get::<T>()`, and `get_all::<T>()` support exact,
optional, and collection queries after construction. `get_all()` orders
results by `order`, ID, and source location.

`register_instance_with` validates its own ID and profile when staging the
instance. Collisions with other definitions are checked at build time after
inactive profiles have been filtered. Factory panics propagate with Rust's
normal panic behavior. On a later construction failure, synchronous builds
stop managed values already returned by factories but do not wait for them;
asynchronous builds stop and wait before returning the build error with any
cleanup failures.

### Replace one complete definition

Applications can replace one binding while capturing runtime state in the
registration callback:

```rust
use std::error::Error;
use std::sync::Arc;
use qubit_ioc::{BindingKey, ContainerBuilder};

fn replace_for_test() -> Result<(), Box<dyn Error>> {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(1_u64))?;
    let fake = Arc::new(7_u64);
    let key = BindingKey::of::<u64>(None);
    builder.replace_definition(key, move |draft| draft.register_instance(fake))?;
    builder.root::<u64>();
    let context = builder.build()?;
    assert_eq!(*context.get::<u64>()?, 7);
    Ok(())
}
```

The callback runs against a temporary builder. It must register exactly one
definition that declares the anchor key. An error leaves the original builder
unchanged. The complete original definition is replaced, including all aliases;
the replacement must declare any aliases that remain needed.

Definitions can use an activation `profile`. Each call to `active_profiles`
replaces the previous set; an empty set activates `default`, and definitions
without a profile always remain active. Configuration reads through `#[value]` or
`#[ConfigurationProperties]` require the `config` feature and a staged
`qubit-config` snapshot. A repeated active `Config` key is reported as a
build-time `BuildError::DuplicateBinding`. See the [English design document](complete-design.md)
for the full lifecycle and option contract.

## Configuration and request choices

Use `Option<Arc<T>>` when a profile may omit a dependency; it resolves to `None` when no candidate is registered. Use `Vec<Arc<T>>` when every candidate is relevant; an empty candidate set resolves to an empty vector. Both declarations remain part of graph validation when they match active definitions. For an exact implementation, attach an ID with `#[inject(id = "storage.primary")]`; missing IDs fail during build and never fall back to `primary`.

With both `macros` and `config` enabled, a field can read one value with `#[value("service.port")]`. Stage the snapshot using `builder.with_config(config)?` before installing the definition. A missing key or a value of the wrong type becomes `BuildError::ConfigReadFailed`, retaining the configuration source and field path.

`#[value]` reads the stored value directly. `ConfigurationProperties` also
deserializes stored values without interpolation and rejects unknown fields by
default. If an application needs interpolation, inject `Config` into a manual
factory and call `Config::get_interpolated`; see
[`examples/config_contract.rs`](../examples/config_contract.rs). These rules
are covered in [config_tests.rs](../tests/config_tests.rs).

For a structured subtree, derive `Deserialize` and use `#[ConfigurationProperties(prefix = "service")]` on a named-field struct. Install that definition and stage the same config snapshot. A missing or invalid property fails construction; the error retains the original deserialization detail. The config macro tests in `tests/config_macro_tests.rs` show the exact setup and successful result.

### Managed factory output paths

`#[bean]` recognizes `Managed<T>` when its path matches the runtime dependency
name. The following source forms are supported:

| Runtime dependency name | Accepted output paths |
| --- | --- |
| `qubit-ioc` | imported `Managed<T>`, `qubit_ioc::Managed<T>`, `::qubit_ioc::Managed<T>` |
| renamed to `ioc` | `ioc::Managed<T>`, `::ioc::Managed<T>` |
| renamed to the keyword `type` | `r#type::Managed<T>`, `::r#type::Managed<T>` |

The same paths work inside `Result<Managed<T>, E>` for sync and async beans.
The macro does not guess that `application::Managed<T>` or a type alias is the
runtime wrapper; those remain ordinary component outputs. Independent consumer
workspaces exercise each path in
[`managed_paths`](../tests/fixtures/managed_paths/).

## Cross-crate application assembly

A provider crate owns component definitions, while the application chooses which providers to install and which services to build. The repository fixture at `tests/fixtures/ioc_cross_crate/` demonstrates this boundary; it is an executable contract test, not a claim about a production deployment.

The provider crate exports `register_ioc(&mut builder)`. The app fixture's `app/src/discovery.rs` defines `assemble(config, profiles)`: it creates a builder, stages the configuration snapshot and active profiles, then calls the provider registration function. Registration only stages definitions; factories still wait for `build()` or `build_all()`.

For a consuming application, the dependency roles are visible in `tests/fixtures/ioc_cross_crate/app/Cargo.toml`: `qubit-ioc`, `qubit-config`, the provider crate and the contracts crate. The fixture test creates a `Config`, sets `fixture.label`, and then calls `assemble(config, &[])`. It builds the graph and queries `AppService`, a concrete repository by ID, and the primary `dyn Repository` binding. The test verifies that concrete and trait queries share the same allocation, and that a bean in the provider crate reads the staged configuration. Run the contract with:

```bash
cargo test --manifest-path tests/fixtures/ioc_cross_crate/Cargo.toml
```

If the configuration subtree is absent, construction returns `BuildError::ConfigReadFailed`; its source chain retains the original `ConfigError`. To activate the optional preview provider, call `assemble(config, &["default", "preview"])`; an inactive provider is absent from the built context. The fixture's integration tests assert both outcomes.

A separate downstream fixture, `rs-execution-services/tests/fixtures/ioc_application_consumer/src/main.rs`, demonstrates the resource lifecycle boundary: it installs managed `ExecutionServices` and `EventBus`, obtains shared services after build, requests stop through `begin_shutdown()`, then awaits `ShutdownHandle::wait()`. These snippets come from different fixtures with different purposes; use them as contract references and keep application-specific configuration and external side effects in the consuming application.

Use an async factory when construction itself must await I/O. `#[bean] async fn` and `register_async_factory` both create async definitions; choose `build_async()` or `build_all_async()` and drive the returned future with the application's executor. Calling synchronous `build()` on a selected async definition returns `BuildError::AsyncRequired` before any factory runs. The app owns executor choice and cancellation policy.

## Errors and diagnostics

| Symptom | Where to look | Action |
| --- | --- | --- |
| `RegistrationError::InvalidBindingId` or `DuplicateDependency` | Registration call or macro definition | Correct the ID grammar or declared dependency list. |
| `BuildError::NoRootsSelected` or `MissingRoot` | Root selection | Add a root or select an active binding. |
| `BuildError::MissingDependency`, `AmbiguousBinding`, or `DependencyCycle` | Validation path and candidate keys | Register the missing type, select an ID or primary, or break the cycle. |
| `BuildError::AsyncRequired` | Build method | Drive `build_async()` or `build_all_async()` with an executor. |
| `BuildError::FactoryFailed` or `ConfigReadFailed` | Source error and construction path | Fix the factory or configuration input. |
| `ResolveError::MissingComponent` or `AmbiguousBinding` | Query type and candidates | Query a built root or use `get_by_id()`. |

Graph errors are returned before user factories run. A factory can still fail
after earlier factories have performed external work. The container cleans up
managed resources already transferred to it; factories or the application own
other side effects. Inspect the preserved source chain of `FactoryFailed`. A factory may only read dependencies declared
at registration, or it receives `BuildAccessError::UndeclaredDependency`.

## Lifetime and limits

The context stores shared `Arc` instances and does not rerun factories on
lookup. Read-only context queries support concurrent sharing through `Arc`.
When shutdown is needed, release all shared context handles, recover the
single owner with `Arc::try_unwrap`, then call `begin_shutdown()`. The
[`context_sharing` example](../examples/context_sharing.rs) demonstrates this
sequence. Components own synchronization of their mutable state. Resource
components can opt in to managed shutdown with `Managed<T>`. The runnable
example starts a worker task inside its factory, sends a stop signal, awaits
the task, and verifies it exited:

```bash
cargo run --example app_lifecycle --no-default-features
```

See [`examples/app_lifecycle.rs`](../examples/app_lifecycle.rs) for its source
and the [lifecycle guide](lifecycle.md) for shutdown and cancellation details.

Create managed resources inside managed factories, which run only after graph
validation. For an external resource that is already running, register its
`Arc<T>` with `register_instance` and keep shutdown ownership in the
application. Do not capture an already-created `Managed<T>` in a factory.

`Managed::new` provides a synchronous stop request; `.with_wait` can add an
asynchronous termination wait. `begin_shutdown(self)` calls all stop actions
in reverse construction order before returning a `ShutdownHandle`.
`wait(&mut self)` awaits waits in that same order and returns every failure.
If a wait future is cancelled, keep the handle and call `wait()` again to
resume that same future. A synchronous `build()` failure stops resources already
returned as `Managed` but cannot await them. An asynchronous `build_async()`
failure calls stop and then waits, preserving cleanup failures alongside the
original build error. If failed construction must wait for already-created
resources before returning, use `build_async()` even when every factory is
synchronous. Cancelling an asynchronous build calls
stop for resources already returned as `Managed`, without waiting, because the caller no longer has a future to receive
errors from. Effects created inside a factory before it returns `Managed<T>`
remain the factory's responsibility.

Dropping the context does not stop resources. External `Arc` clones can keep a
value alive after shutdown; shutdown requests termination but cannot
revoke those clones. Stop callback panics are collected as Stop failures.
Panics while creating a wait future or polling it are collected as Wait
failures, and remaining waits still run. `panic = "abort"` and panics while
dropping a cleanup future cannot be caught. Factory panics during construction
also propagate.

The former managed-instance registration entry points have been removed.
Create managed resources inside `register_managed_factory` (or the equivalent
managed `#[bean]` factory); if a resource must exist before registration, use
`register_instance` and let the application own its shutdown actions.

There are no prototype or request scopes, hot reload, automatic lifecycle
management for unmanaged components, circular proxies, or dynamic-library discovery. Struct macros support named
fields and unit structs; use a manual factory for other shapes. Runtime
reflection does not construct components. `qubit-spi` handles provider
selection and fallback separately.

Continue with the [README](../README.md), [中文用户手册](user_guide.zh_CN.md),
or run `cargo doc --no-deps --open` for API documentation.

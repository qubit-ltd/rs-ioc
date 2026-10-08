# qubit-ioc User Guide

[中文用户手册](user_guide.zh_CN.md) · [README](../README.md)

This guide is for Rust application authors using `qubit-ioc` 0.3.0. It explains
how to assemble application-wide shared components,
diagnose startup failures, and manage their lifetime. Rust 1.94 or newer is
required by the package manifest.

## Conceptual model

| Term | Meaning |
| --- | --- |
| Definition | A staged instance or factory; registration does not run a factory. |
| Binding | A Rust type and optional, case-sensitive ID that identify a component. |
| Dependency | A request declared by a factory or generated from a macro field. |
| Root | A requested component whose dependency closure `build()` constructs. |
| Application | The unique lifecycle owner returned by a successful build. |
| Context | The cloneable read-only `ApplicationContext` borrowed from `application.context()`. |

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

Add `qubit-ioc` to the application dependencies:

```toml
[dependencies]
qubit-ioc = { version = "0.3", default-features = false }
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

    let application = builder.build()?;
    let context = application.context();
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
qubit-ioc = { version = "0.3", default-features = false, features = ["macros"] }
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
    let application = builder.build()?;
    let context = application.context();
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
return `Managed::asynchronous(value, abort, wait)` so shutdown can confirm its
termination. Use `Managed::synchronous(value, stop)` only when successful stop
already means termination is complete.
Configure a bounded `WaitPolicy` before building any selected managed graph.
Retain the returned `Application`; clones of `application.context()` may remain
alive while it shuts down. For normal exit, call
`application.begin_shutdown(ShutdownMode::Graceful)` and await the handle's
`wait()`; failure and cancellation request Immediate abort. The [lifecycle
guide](lifecycle.md) shows a complete bounded integration function that waits
on both build-failure cleanup and normal shutdown. The [managed worker
example](../examples/app_lifecycle.rs) runs the same owner/handle contract.

## Typed factories and application shutdown budgets

Manual synchronous and asynchronous factories have four typed registration
methods: `register_injected_factory`, `register_injected_managed_factory`,
`register_injected_async_factory`, and
`register_injected_managed_async_factory`. Their argument tuple generates the
dependency requests: `()` requests nothing, `Arc<T>` requires one binding,
`Option<Arc<T>>` allows none, and `Vec<Arc<T>>` requests all candidates. Tuples
with zero through eight arguments are supported. For named IDs or custom
dependency requests, use `register_async_factory` or
`register_managed_async_factory` and provide the requests explicitly.

An async factory selected by the build graph requires `build_async()` or
`build_all_async()`, which the application must poll on its chosen executor.
Synchronous `build()` reports `BuildError::AsyncRequired` before invoking any
factory. Create managed resources after graph validation where possible;
side effects performed before an async factory returns `Managed<T>` remain that
factory's responsibility if construction later fails or is cancelled.

For example, this complete function uses a required `String` dependency and
builds all active definitions asynchronously:

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

For managed shutdown, `WaitPolicy::bounded_with_total(grace, termination,
total, timer)` adds an application-wide budget. It starts when `wait()` is
first polled; cancellation preserves it. Expiry requests abort for unconfirmed
components and appears in `ShutdownReport::overall_failure()`. A total deadline
cannot interrupt synchronous blocking work. See the [lifecycle guide](lifecycle.md)
for the full contract.


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
an exact binding. With roots selected, `build()` validates only the reachable
dependency closure by default. Set `validation_scope(ValidationScope::AllActive)`
to check every active definition before constructing that closure;
`build_all()` checks and constructs every active definition and can build an
empty graph. `AllActive` does not run unselected factories or require an
unselected async factory or managed factory to match the selected build mode or
`WaitPolicy`. Both build methods have asynchronous counterparts. A selected
graph with an asynchronous factory requires `build_async()` or `build_all_async()`
and an executor supplied by the application.

Use `AllActive` for an application assembly check that should reject an
unselected but invalid active definition before any factory runs. Keep the
default `Reachable` for a deliberately selected profile or tenant subgraph.
For example, this unselected definition has a missing dependency; the root
build succeeds under `Reachable`, while `AllActive` rejects it before running
the root factory:

```rust
use std::sync::Arc;
use qubit_ioc::{BuildError, ContainerBuilder, Dependency, ValidationScope};

fn builder() -> ContainerBuilder {
    let mut builder = ContainerBuilder::new();
    builder.register_factory::<u32, _>(&[], |_| Ok(Arc::new(1)))
        .expect("register root");
    builder.register_factory::<u64, _>(&[Dependency::of::<i16>()], |_| Ok(Arc::new(2)))
        .expect("register unselected definition");
    builder.root::<u32>();
    builder
}

fn main() {
    assert!(builder().build().is_ok());
    let failure = builder().validation_scope(ValidationScope::AllActive)
        .build().err().expect("missing dependency");
    assert!(matches!(failure.cause(), BuildError::MissingDependency { .. }));
}
```

`build_all()` is for actually constructing both active definitions; here it
would also reject the missing dependency. `AllActive` alone does not execute
the unselected factory.

If several bindings have the same Rust type, an unnamed request selects the
sole candidate or the sole `primary` candidate. Otherwise it is ambiguous.
Use a valid ID for exact selection; each dot-separated ASCII segment must
start with a letter and contain only letters, digits, or underscores.
`get_by_id::<T>()`, `try_get::<T>()`, and `get_all::<T>()` support exact,
optional, and collection queries after construction. `get_all()` uses the immutable order precomputed at context publication:
`order`, ID, source location, then registration position.

`register_instance_with` validates its own ID and profile when staging the
instance. Collisions with other definitions are checked at build time after
inactive profiles have been filtered. Factory panics propagate with Rust's
normal panic behavior. If construction fails after managed values have been
transferred to IoC, the original build modes request abort and return `BuildFailure`
without waiting. The application that owns this failure decides when to observe
cleanup. Keep `BuildFailure` as a typed application-error variant until
`wait_cleanup()` has completed, or retain the `ShutdownHandle` returned by
`take_cleanup()` or `into_parts()` and wait on it. Keeping only the `BuildError`
or cause discards the cleanup observation handle. `wait_cleanup()` borrows the
failure, preserves its original cause, and can be called again after its future
is cancelled.

### Replace one complete definition

Applications can replace one binding while capturing runtime state in the
registration callback:

```rust
use std::error::Error;
use std::sync::Arc;
use qubit_ioc::{BindingKey, ContainerBuilder, Definition};

fn replace_for_test() -> Result<(), Box<dyn Error>> {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(1_u64))?;
    let fake = Arc::new(7_u64);
    let key = BindingKey::of::<u64>(None);
    builder.replace_definition(key, move |draft| {
        let replacement = Definition::<u64>::builder().instance(fake).build()?;
        draft.register_definition(replacement)
    })?;
    builder.root::<u64>();
    let application = builder.build()?;
    let context = application.context();
    assert_eq!(*context.get::<u64>()?, 7);
    Ok(())
}
```

The callback runs against a temporary builder. It must register exactly one
definition that declares the anchor key. `Definition::builder()` also accepts
`binding`, `dependencies`, sync/async and managed factories, and `bind` for
trait aliases. `build()` validates the complete definition without running
user factories; `register_definition` stages it atomically. The macros use
this public core rather than the removed hidden definition protocol. An error leaves the original builder
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

If the configuration subtree is absent, construction returns `BuildFailure` with
`BuildError::ConfigReadFailed` as its cause; its source chain retains the original `ConfigError`. To activate the optional preview provider, call `assemble(config, &["default", "preview"])`; an inactive provider is absent from the built context. The fixture's integration tests assert both outcomes.

A separate downstream fixture, `rs-execution-services/tests/fixtures/ioc_application_consumer/src/main.rs`, demonstrates the resource lifecycle boundary: it installs managed `ExecutionServices` and `EventBus`, obtains shared services after build, uses `Application::begin_shutdown(ShutdownMode::Graceful)` on normal exit,
`Immediate` on failure, and awaits `ShutdownHandle::wait()`. Its EventBus adapter
uses `request_shutdown` plus a ticket and `wait_async()`: synchronous
`EventBus::shutdown(Immediate)` would still wait in a stop callback. These snippets come from different fixtures with different purposes; use them as contract references and keep application-specific configuration and external side effects in the consuming application.

Use an async factory when construction itself must await I/O. `#[bean] async fn`,
`register_injected_async_factory`, and `register_injected_managed_async_factory`
cover typed dependency tuples; use `register_async_factory` or
`register_managed_async_factory` when requests need IDs or custom metadata.
Choose `build_async()` or `build_all_async()` and drive the returned future with
the application's executor. Calling synchronous `build()` on a selected async definition returns `BuildFailure`
with `BuildError::AsyncRequired` as its cause before any factory runs. The app owns executor choice and cancellation policy.



## Build failures and rollback observation

The original `build()` and `build_async()` return `BuildFailure` immediately after requesting abort for managed resources already transferred to the container. Use these methods when immediate return matters; keep the `BuildFailure` so cleanup can be observed later with `wait_cleanup()`. That method borrows the failure, preserves the original cause, and can be called again if its future is cancelled. `take_cleanup()` and `into_parts()` transfer cleanup ownership. Keeping only the `BuildError` or its cause discards the cleanup observation handle.

The four settled methods are async and return `Result<Application, SettledBuildFailure>`. For synchronous factories use `build_settled()` or `build_all_settled()`; for asynchronous factories use `build_async_settled()` or `build_all_async_settled()`. On normal completion they wait for rollback observation before returning. `SettledBuildFailure::cause()` retains the original `BuildError` as the error source, while `cleanup_report()` returns an optional final `ShutdownReport`. The report may be absent when graph/preflight validation failed before managed resources were created; when present, cleanup can fail or remain incomplete.

This example makes `Startup` fail after `Worker` has been constructed, then observes both successful and failed cleanup reports:

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

Here `cleanup_report()` is present because `Worker` was built before the `Startup` factory failed. `show_cleanup_reports()` reaches both a successful report and a failed report. A graph/preflight failure instead has no managed cleanup and therefore no report.

Cancelling or dropping a settled future can interrupt its wait. Cancellation requests best-effort abort but does not guarantee rollback observation finishes. Keeping the original `BuildFailure` instead lets the application continue observing cleanup with `wait_cleanup()`. Neither a timer nor cancellation can preempt synchronous blocking callbacks or a blocking future poll.

For the whole managed shutdown lifecycle—including total budgets, tickets, and callback ordering—see the [lifecycle guide](lifecycle.md) and [managed adapter guide](managed-adapters.md). `WaitPolicy::bounded_with_total(grace, termination, total, timer)` starts its total timer when `ShutdownHandle::wait()` is first polled; that deadline persists across cancellation and abort upgrades. It cannot interrupt blocking work.

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

`Application` owns lifecycle while `application.context()` provides immutable
shared queries. Clone the query handle for concurrent readers; no
`Arc::try_unwrap` or release of every query clone is needed to shut down.
The [context-sharing example](../examples/context_sharing.rs) exercises this
owner/context split. `ApplicationContext::state()` exposes lifecycle progress for
observation; it does not gate context lookup. Context lookup can still succeed
after shutdown starts, and does not establish that a resource still admits work.

Create managed resources inside managed factories, after graph validation. For
an already-running external resource, register its `Arc<T>` and retain shutdown
ownership in the application. Managed graphs need an explicit bounded
`WaitPolicy` in real applications. Choose `Managed::synchronous` when a
successful stop callback confirms termination, or `Managed::asynchronous` to
pair a non-blocking abort request with a required termination wait. Select
graceful behavior at construction with `Managed::synchronous_with_graceful`
or `Managed::asynchronous_with_graceful`; ticket-aware constructors keep the
request result paired with its wait callback. The selected constructor fixes
the graceful callback. The
[managed adapter guide](managed-adapters.md) explains these contracts and
ticket ownership. At normal exit choose Graceful and await the handle.
Graceful requests begin on the first `wait` poll and run consumer-by-consumer before
their dependencies. Immediate requests all aborts before returning the handle.
The [lifecycle guide](lifecycle.md) supplies the complete failure and business
exit flow, and [app_lifecycle](../examples/app_lifecycle.rs) runs a worker.

The original build variants return `BuildFailure` immediately after requesting abort
for transferred managed values. The application owns cleanup observation:
inspect `cause()`, then await optional cleanup with `wait_cleanup()` or take its
handle with `take_cleanup()` or `into_parts()`. Keep `BuildFailure` as a typed
application-error variant until cleanup completes, or keep the extracted
`ShutdownHandle` and wait on it. Keeping only the `BuildError` or cause loses
the cleanup observation handle.
`wait_cleanup()` borrows the failure, so cancelling it and calling it again
resumes the same cleanup wait. Dropping the owner,
an untransferred `Managed`, or a shutdown handle requests abort but never waits;
query context Drop has no shutdown action. Dropping the failure or cancelling
an async build never waits. Unwind panic in a factory propagates. The factory
is responsible for side effects created before it returns `Managed`.

Cancelling `ShutdownHandle::wait()` retains its active future and deadline;
calling it again on the same handle resumes. Bounded deadlines require the
timer to be driven and cannot interrupt blocking callbacks or future polls.
`ShutdownReport::incomplete()` records unconfirmed termination, not a killed
resource. Callback errors stay in the report even if all waits finish.
External `Arc` clones may keep object memory alive after shutdown.

The former managed-instance registration entry points have been removed.
Use `register_managed_factory` or a managed `#[bean]` factory. The public
`Definition::builder()` and `register_definition` are available for complete
custom registrations and trait aliases. Collections use the order precomputed
at publication; construction and cleanup ordering are separate.

There are no prototype or request scopes, hot reload, automatic lifecycle
management for unmanaged components, circular proxies, or dynamic-library
discovery. Struct macros support named fields and unit structs; use a manual
factory for other shapes. Runtime reflection does not construct components.
`qubit-spi` handles provider selection and fallback separately.

Continue with the [README](../README.md), [中文用户手册](user_guide.zh_CN.md),
or run `cargo doc --no-deps --open` for API documentation.

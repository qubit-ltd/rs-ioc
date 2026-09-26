# qubit-ioc User Guide

[中文用户手册](user_guide.zh_CN.md) · [README](../README.md)

This guide is for Rust application authors using the `qubit-ioc` 0.1.0 source
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
qubit-ioc = { version = "0.1", path = "../rs-ioc", default-features = false }
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

## Choosing definitions and build scope

With default features, `#[Component]`, `#[Service]`, and `#[Repository]`
generate definitions. `#[bean]` generates a factory definition. Call
`ApplicationContext::builder().discover()?` to import definitions linked into
the final binary, or `builder.install::<T>()?` to install a generated definition
explicitly. Declare a trait alias with `bind = dyn Trait`; a separate `impl`
block alone does not create that binding.

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

Definitions can use an activation `profile`. The default profile is active
when none is chosen; `active_profiles(&["name"])?` selects another set before
building. Configuration reads through `#[value]` or
`#[ConfigurationProperties]` require the `config` feature and a staged
`qubit-config` snapshot. Generate the public API with `cargo doc --no-deps` and see the
[design document](complete-design.zh_CN.md) for the full option set.

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
after earlier factories have performed external work; inspect the preserved
source chain of `FactoryFailed`. A factory may only read dependencies declared
at registration, or it receives `BuildAccessError::UndeclaredDependency`.

## Lifetime and limits

The context stores shared `Arc` instances and does not rerun factories on
lookup. Components own synchronization of their mutable state. Releasing the
context does not force resources to close while other `Arc` handles remain;
call application shutdown methods explicitly. Cancelling an asynchronous
build stops unstarted factories but does not undo completed external effects.
Factory panics propagate as Rust panics.

There are no prototype or request scopes, hot reload, lifecycle hooks,
circular proxies, or dynamic-library discovery. Struct macros support named
fields and unit structs; use a manual factory for other shapes. `reflect`
currently adds no construction behavior. `qubit-spi` handles provider
selection and fallback separately.

Continue with the [README](../README.md), [中文用户手册](user_guide.zh_CN.md),
or run `cargo doc --no-deps --open` for API documentation.

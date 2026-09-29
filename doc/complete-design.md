# qubit-ioc Current Design

> This document describes the implemented design and public contracts of
> `qubit-ioc` 0.2.0 release candidate. For usage, see the [English user guide](user_guide.md) or
> [中文用户手册](user_guide.zh_CN.md). Historical design notes remain available
> in [the kernel draft](design.zh_CN.md) and [the annotation draft](annotation-design.zh_CN.md).
> The implementation and tests define behavior; this document does not promise
> capabilities absent from the public API.

## 1. Purpose and boundaries

`qubit-ioc` stages application-wide shared components, resolves and validates
their dependency graph, and invokes factories in dependency order during
application startup. A successful build publishes a read-only
`ApplicationContext`. Definitions can come from attribute macros or explicit
registration. A macro-generated definition is not installed automatically:
the application or provider must call `install::<T>()` or its own
`register_ioc(&mut builder)` function.

The built context supports concurrent read-only queries through `Arc`.
Shutdown remains a single-owner operation that consumes the context after all
shared query handles have been released.

The runtime handles bindings, dependency selection, construction order,
diagnostic paths, and explicit managed shutdown. `qubit-spi` handles provider
selection and fallback. The container has no request or prototype scopes, hot
reload, circular proxies, automatic discovery, reflective construction, or
automatic shutdown for unmanaged resources.

## 2. Packages and features

The workspace contains the `qubit-ioc` runtime and `qubit-ioc-macros` proc
macro crate. The package uses Rust 2024 and requires Rust 1.94. Default
features are `macros` and `config`.

| Feature | Capability |
| --- | --- |
| `macros` | Exports `Component`, `Service`, `Repository`, `Configuration`, `ConfigurationProperties`, and `bean`. |
| `config` | Integrates `qubit-config` snapshots and typed configuration reads. |

These features can be enabled independently. `#[value]` and
`#[ConfigurationProperties]` require both. Manual registration works with
`default-features = false`.

## 3. Definitions and bindings

A **definition** stages one instance or factory. A **binding** is identified
by its Rust type and optional case-sensitive ID. IDs use dot-separated ASCII
segments, each starting with a letter and followed by letters, digits, or
underscores. A concrete component can expose a trait-object binding with
`bind = dyn Trait`; a separate `impl Trait for Type` does not create one. The
concrete key and its aliases share the same `Arc` allocation.

Macros support named-field and unit structs, plus synchronous and asynchronous
free-function factories. Fields and parameters declare required, exact-ID,
optional, or collection requests. Unsupported data shapes can use manual
factories. `cfg` and supported nested `cfg_attr(..., cfg(...))` activation
conditions are projected to generated registration and factory code. A helper
attribute such as `inject` or `value` inside `cfg_attr` is rejected; apply the
helper directly to the field.

`BindingOptions` can specify ID, `primary`, `order`, and `profile`. Profiles
filter definitions before duplicate and dependency validation. An unprofiled
definition is always active. Calling `active_profiles` replaces the previous
set; an empty set activates `default` plus unprofiled definitions. When several
bindings satisfy an unnamed single-value request, exactly one `primary` is
required. Exact-ID requests ignore `primary`. `primary` and `order` on trait
aliases do not change selection metadata on the concrete binding.

`replace_definition(anchor, callback)` stages a whole replacement in a
temporary builder. The callback must register exactly one definition that
contains the anchor once. Success replaces all bindings from the earlier
definition, including aliases; the replacement must re-declare any aliases it
still needs. A callback or validation error leaves the original builder
unchanged.

Function beans generate a default PascalCase function-name marker with a
`Bean` suffix; `#[bean(marker = CustomFactory)]` can override it. A
`#[Configuration]` module exposes `register_ioc(&mut builder)`, which stages
its beans and needs only `macros`. The
[function bean example](../examples/readme_beans.rs) shows installation, roots,
and reads; configuration-value reads still require both features.

## 4. Registration, graph validation, and construction

The build lifecycle is:

```text
registration → active definitions and root closure → graph validation
             → sync-build async preflight → construction
             → context publication → explicit shutdown
```

Registration does not call user factories. `build()` constructs the closure
of one or more required roots and needs at least one root. `build_all()`
constructs every active definition and permits an empty graph. Async variants
are `build_async()` and `build_all_async()`. If the selected graph contains an
async factory, a synchronous build returns `AsyncRequired` before running any
factory. The application supplies and drives its executor; synchronous and
async factories can coexist in an async build.

Validation resolves roots and requests after profile filtering, then checks
missing or ambiguous dependencies, duplicate active keys, primary selection,
and cycles. Neither a factory nor an alias projector runs until the complete selected
graph validates and, for synchronous construction, the async preflight passes.
Absent optional requests resolve to `None`, and absent collection requests to
an empty vector; matching requests still participate in validation. Factories
run serially with dependencies before consumers, preserving registration and
declaration order for ties. A failure never publishes a partial context.

Diagnostic paths retain one deterministic predecessor per selected binding,
then reconstruct the full path only when reporting an error. Multiple roots
keep their declared order. A cycle path keeps its root prefix and the actual
cycle suffix, including the repeated binding. Bindings added because one
definition is selected as a unit retain a definition-member provenance in the
path. `BuildContext` grants access only to requests declared and resolved by
the graph; an undeclared query returns `BuildAccessError`.

The published context builds immutable type and exact-key indexes. Single-value
selection and collection sorting still take place during each query.
`BuildContext::get_all` and `ApplicationContext::get_all` order collections by
ascending `order`, ID, and source location; complete ties retain registration
order. Diagnostic candidate and available-binding lists retain registration
order.

## 5. Configuration and integration boundaries

With `config` enabled, `with_config(config)` stages a shared snapshot. A second
active `Config` binding with the same key is a build-time `DuplicateBinding`;
it is not rejected by `with_config` itself. `#[value]` reads a scalar and
`#[ConfigurationProperties]` deserializes a subtree. A missing snapshot,
missing value, or conversion error is reported during validation or
construction with the original configuration source and component path.
`#[value]` and structured deserialization do not interpolate values;
structured deserialization rejects unknown fields by default. A manual factory
can call `Config::get_interpolated` when interpolation is required.

Provider crates should expose an explicit `register_ioc(&mut builder)` entry
point. The application chooses which providers to install. The
`tests/fixtures/ioc_cross_crate/` fixture checks provider registration,
trait aliases, profiles, configuration error sources, and manual assembly
without default features. It is a contract test, not evidence of production
adoption.

IoC can coexist with `qubit-spi`: SPI selects an implementation within one
service family, while IoC assembles wider application dependencies and startup
order. IoC does not depend on SPI or discover providers implicitly.

## 6. Errors and lifecycle

Errors identify their stage. `RegistrationError` covers invalid IDs, profiles,
and repeated declared requests. `BuildError` covers roots, candidates, graph
validation, async requirements, configuration reads, and factory failures.
`ResolveError` covers queries after publication. `ShutdownError` aggregates
resource cleanup failures. Factory and configuration errors retain their
source chain; graph and construction errors retain binding sources and
dependency paths when available.

Create managed resources inside a managed factory, which only runs after graph
validation. A selected synchronous managed factory can run in a synchronous or
asynchronous build; a later factory failure rolls back resources already
returned to the container. `Managed<T>` records a synchronous stop callback and optional async
wait callback. Applications explicitly call `context.begin_shutdown()` and
then await `ShutdownHandle::wait()`. Stops run in reverse construction order,
followed by waits in that order. Stop errors and unwind panics are recorded and
do not prevent later callbacks. Panics while creating or polling a wait future
are recorded as wait failures. `panic = "abort"` and panics while dropping a
cleanup future cannot be caught.

`Managed<T>` and `ShutdownHandle` carry type-level `must_use` guidance.
Return managed values to the container; dropping an untransferred `Managed`
does not invoke cleanup. Await the shutdown handle, or explicitly use
`drop(context.begin_shutdown())` to abandon waiting after stop requests run.

If a wait is cancelled, keep the handle and call `wait()` again to resume the
same future. Dropping the handle abandons unfinished waits after stops have
run. A synchronous build failure stops resources already returned as `Managed` but does not
wait. An async build failure stops and waits, preserving cleanup failures with
the original build error. Cancelling an async build calls stop once per resource already returned as
`Managed`, in reverse construction order, without waiting. Side effects made by a factory before it returns a
`Managed<T>` remain that factory's responsibility. Dropping a context does not
automatically stop managed resources, and external `Arc` clones can outlive
shutdown.

See the runnable [managed worker example](../examples/app_lifecycle.rs) and
the [lifecycle guide](lifecycle.md) for a complete task stop-and-join flow.

## 7. Verification sources

The public contracts are exercised by `tests/`, macro tests and consumer
fixtures. They cover explicit installation, root selection, IDs, profiles,
replacement, diagnostic paths, sync and async construction, cancellation,
shared trait aliases, configuration error sources, and managed shutdown order
and recovery. Exact public signatures are defined by the generated Rust API
documentation.


Downstream verification keeps separate manifests and locks:
`tests/fixtures/application_consumer/` fixes the historical dependency snapshot;
`tests/fixtures/application_consumer_current/` holds a reviewed current snapshot.
Both CI lanes run `check --locked`, `test --locked`, and `run --locked`, recording
actual checkout SHAs, the Rust version, and the lock's SHA256. The current lane
must pin an accepted commit containing the latest lifecycle fixture. The
historical lane only proves contracts present in its selected source, rather
than inclusion of new tests. Upgrade dependency constraints, SHAs, and locks
together; neither lane follows a floating branch automatically.

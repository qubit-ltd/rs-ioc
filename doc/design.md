# qubit-ioc Kernel Design

> **Historical draft; not a current API guide.** The current contract is [complete-design.md](complete-design.md), and current usage is in the [English user guide](user_guide.md). Reflection integration and the `reflect` feature described here are not implemented. See the [Chinese original](design.zh_CN.md).

> Status: historical design draft, written before implementation. It specifies a proposed contract for explicit registration and container construction. The contemporaneous annotation draft records an application-layer proposal, not the current entry point. Examples are historical illustrations only; they do not promise compilable or runnable APIs available in `src/lib.rs`.

## 1. Purpose and scope

`qubit-ioc` is a component assembly container for application startup. The application explicitly registers component instances or factories. The container validates dependencies, constructs all components in dependency order, and then exposes a read-only, shareable component set. Missing dependencies, cycles, and construction failures can be found before the application handles business requests.

A typical application registers a database pool and configuration, then a repository that depends on the pool, and finally a service that depends on the repository. After a successful build, request handlers obtain shared service handles; they do not call container factories while handling requests.

The kernel's proposed goals are:

1. Explicitly register existing instances, synchronous factories, and asynchronous factories.
2. Identify components by Rust type and optional binding ID, with at most one binding per exact key.
3. Require factories to declare dependencies and validate missing dependencies and cycles before construction.
4. Construct every component in deterministic order, invoking each factory at most once per build.
5. Retrieve `Arc<T>` after a successful build so consumers can share components.
6. Return structured errors containing component types, dependency paths, and original errors.
7. Avoid binding the kernel to Tokio or another async executor.

The kernel does not handle request scopes, transient objects, runtime binding changes, dynamic library loading, startup/shutdown hooks, or lazy proxies for cycles. A procedural macro can generate ordinary factory implementations for field injection; `qubit-config` handles configuration-file parsing.

## 2. Boundary with qubit-spi

`qubit-spi` registers, selects, creates, and falls back among candidate providers within one service family. `qubit-ioc` manages dependencies and shared lifetimes across different components. They compose:

```text
application config ─┐
                    ▼
                qubit-ioc ─────► shared business components
                    │                    ▲
                    │ inject registry    │ selected backend used during construction
                    ▼                    │
          qubit-spi ProviderRegistry ────┘
```

An application can register `ProviderRegistry<S>` directly as a component, or call its `resolve()` / `create_configured()` from an IoC factory and register the result as another component. IoC does not interpret `ProviderSelection`, duplicate SPI candidate ordering or fallback state, or require `qubit-spi` as a direct dependency. The crates can be used independently; an application constructor provides interoperability.

For example, an `EventBusRegistry` can register implementations during application configuration and then be sealed and registered as an instance. A factory creating `EventBus` declares dependencies on that registry and `EventBusConfig`. If SPI creation fails, the factory returns the original error as its source, and IoC adds the outer component dependency path. IoC does not select another provider after SPI fails, because that would duplicate SPI's fallback policy.

## 3. Core concepts and proposed public API

| Concept | Responsibility |
| --- | --- |
| `ContainerBuilder` | Collects bindings and builds the container; consumed by the build. |
| `Dependency` | Explicitly declares a component type required by a factory. |
| `BuildContext` | Reads already-constructed, declared dependencies inside a factory. |
| `ApplicationContext` | Read-only component set after construction, shareable by the application. |
| `RegistrationError` | Registration-stage errors such as duplicate bindings. |
| `BuildError` | Missing dependencies, cycles, async-build requirement, or factory failure. |

The following is a **historical illustrative API sketch**, not a statement of current API availability. Final signatures would be determined by Rust type checking during implementation; this draft specifies the intended behavior:

```rust,ignore
let mut builder = ContainerBuilder::new();

builder.register_instance::<AppConfig>(Arc::new(config))?;
builder.register_instance::<DbPool>(Arc::new(pool))?;

builder.register_factory::<UserRepository>(
    [Dependency::of::<DbPool>()],
    |context| {
        let pool = context.get::<DbPool>()?;
        Ok(Arc::new(UserRepository::new(pool)))
    },
)?;

builder.register_factory::<UserService>(
    [Dependency::of::<UserRepository>(), Dependency::of::<AppConfig>()],
    |context| {
        let repository = context.get::<UserRepository>()?;
        let config = context.get::<AppConfig>()?;
        Ok(Arc::new(UserService::new(repository, config)))
    },
)?;

builder.root::<Application>();
let context = builder.build()?;
let service: Arc<UserService> = context.get::<UserService>()?;
```

In the proposed API, `register_instance::<T>` accepts `Arc<T>`, and `register_factory::<T>` returns `Result<Arc<T>, BoxError>`, where `BoxError` is a transferable original error. `T: ?Sized + Send + Sync + 'static`, allowing explicit registration of `Arc<dyn Trait>` and retrieval with `get::<dyn Trait>()`. The container does not automatically convert a concrete type into a trait binding; the application must choose the bound interface explicitly.

`ApplicationContext::get::<T>()` returns `Result<Arc<T>, ResolveError>`; `get_by_id::<T>(id)` requests an exact binding ID. `try_get::<T>()` returns `Result<Option<Arc<T>>, ResolveError>`: absence is `None`, while ambiguity remains an error. `get_all::<T>()` returns all shared bindings of that type, ordered by explicit sort value, binding ID, and source. `BuildContext` provides the same single, optional, and collection queries, but permits only requests declared by the factory. Container queries find constructed results and never trigger factories.

### 3.1 Async factories

Async factories use an owned `BuildContext` and a sendable boxed future so the future can access constructed dependencies across `.await` without borrowing builder state. This is a **historical illustrative sketch**:

```rust,ignore
builder.register_async_factory::<DbPool>(
    [Dependency::of::<DbConfig>()],
    |context| Box::pin(async move {
        let config = context.get::<DbConfig>()?;
        let pool = DbPool::connect(&config.url).await?;
        Ok(Arc::new(pool))
    }),
)?;

let context = builder.build_all_async().await?;
```

`build_async()` accepts both synchronous and asynchronous factories; `build()` accepts only instances and synchronous factories. If the builder contains an async factory, `build()` returns `BuildError::AsyncRequired` after graph validation but before calling any factory. Thus missing dependencies and cycles take precedence. Both build methods consume the builder. Async construction does not create its own runtime; the application supplies the executor.

## 4. Registration and identity

A binding is identified by `(TypeId::of::<T>(), Option<BindingId>)`. Errors and logs also retain `type_name::<T>()` for readability. Binding IDs are validated at registration and their namespace is scoped to each Rust type. Registering the same exact key twice returns `RegistrationError::DuplicateBinding` and does not replace the earlier binding. A query without an ID selects the sole candidate when there is one; with multiple candidates it selects the primary only if exactly one is marked primary; otherwise it returns `AmbiguousBinding`. An ID query matches only that ID. Multiple primary bindings of one type are errors before construction. Registration order never silently chooses among implementations.

`BindingId` follows the validation rules of `#[Entity(id = "...")]` in `rs-model-metadata`: a nonempty dot-separated string whose segments match `[A-Za-z][A-Za-z0-9_]*`. For example, `example.repository.postgres` is valid; `example..postgres`, `1example.repo`, and `example.repo-name` are invalid. IDs are case-sensitive, are not normalized, and are not generated from Rust type names or registration sources. `None` means no ID; `Some(id)` must be valid. Macro literals are checked at compile time, while explicitly registered IDs are checked during registration; errors include the original value and source. The same ID can be used for different Rust types, but cannot be registered twice for one type.

A concrete component and a `dyn Trait` interface binding are separate keys. A macro may generate an alias binding from the interface key to the concrete component so both queries return the same underlying `Arc`. The interface alias participates in the dependency graph; its ID and primary designation belong to the interface binding. Runtime selection among multiple SPI backends remains the responsibility of `qubit-spi`.

`register_instance` means the application has already created the object; no factory is called. An instance may own resources outside container management: the container manages only the `Arc` handle it receives and does not claim full ownership of the resource lifetime. The proposed low-level registration API also needs explicit metadata for IDs, primary status, and sort value, shared by macro-generated and hand-written assembly. Defaults are no ID, not primary, and sort value `0`.

Factories declare dependencies with `Dependency::of::<T>()`, `Dependency::with_id::<T>(id)`, `Dependency::optional::<T>()`, or `Dependency::all::<T>()`. Each request records target type, optional binding ID, and single/optional/collection cardinality. Dependency lists retain declaration order; duplicate request keys are rejected during registration to avoid ambiguity in the graph and diagnostics. `BuildContext::get::<T>()` and `get_by_id::<T>()` can access only dependencies declared by the current factory. An undeclared access returns `BuildAccessError::UndeclaredDependency`, even when that type happens to have been constructed. This keeps ordering and missing-dependency diagnostics aligned with the values available to the factory.

## 5. Build algorithm

Construction has three stages: validation, ordering, and execution.

1. **Validation:** Resolve each dependency request by type, binding ID, and primary-candidate rules. Check missing required dependencies, ambiguity, and multiple primary bindings before construction. Optional and collection requests with no candidates add no graph edges. Then use DFS visitation states to detect cycles. Errors include a deterministic path, such as `HttpServer → UserService → UserRepository → DbPool`; a cycle path begins and ends with the same component.
2. **Ordering:** Produce a topological order, with every dependency before its consumer. Independent components are constructed in registration order, and dependencies of one factory are traversed in declaration order. Registration order is stored in the builder; it must not depend on `HashMap` iteration.
3. **Execution:** Instance bindings enter the constructed set directly. A synchronous factory is called once; async factories are awaited sequentially. Each factory receives a `BuildContext` containing only its declared dependency handles. `ApplicationContext` is returned only after every component succeeds.

The first version constructs **serially in topological order**, even when components are independent. This makes side-effect order, error ownership, and cancellation boundaries predictable. Parallel construction could be designed separately if startup time later warrants it, without changing dependency semantics.

Validation must finish before the first factory runs; missing dependencies or cycles cannot leave some factories already executed. If a factory fails, no partially built container is returned. Constructed `Arc`s are released normally when build state is dropped. The container cannot roll back completed external side effects or cancel independent tasks started by a factory.

## 6. Storage, sharing, and lifetime

Internally, type-erased `Arc<T>` handles can be indexed by `(TypeId, Option<BindingId>)`. Erasure must allow recovery using the same type registered, including `T = dyn Trait`. The implementation should distinguish an internally erased handle from the user-facing `Arc<T>` so each query does not reconstruct a component or copy `T`.

After construction, `ApplicationContext` is immutable and `get()` only clones an `Arc`. Repeated queries for the same exact binding key refer to the same component instance. Components choose their own locks, atomics, or message passing for mutable internal state. The container provides no `get_mut` and does not propagate mutable borrows across threads.

Dropping the container releases the component handles it owns; an `Arc` retained by a caller can keep a component alive. Therefore, the first version promises no deterministic resource shutdown order and has no automatic close callback. The application explicitly invokes domain APIs before dropping the container when resources need active shutdown.

If the `build_async()` future is cancelled while waiting for a factory, later components are not constructed and local build state is dropped; external side effects that already occurred are not automatically undone. Factory panics propagate according to Rust's default behavior and are not wrapped as ordinary `BuildError`s. This applies to both synchronous and asynchronous factories.

## 7. Error model

Use matchable error variants rather than panics for ordinary graph errors:

| Stage | Error | Required information |
| --- | --- | --- |
| Registration | `DuplicateBinding` | Conflicting type and original registration location or order. |
| Registration | `DuplicateDependency` | Factory type and repeated dependency type. |
| Validation | `MissingDependency` | Full path from the root to the missing type. |
| Validation | `AmbiguousBinding` | Requested type, available binding IDs, and primary status. |
| Validation | `MultiplePrimaryBindings` | Requested type and sources of all conflicting primary bindings. |
| Validation | `DependencyCycle` | Cycle path whose first and last nodes match. |
| Validation | `AsyncRequired` | At least one asynchronous binding type. |
| Construction | `FactoryFailed` | Failed component, path to it, and original source. |
| Factory access | `UndeclaredDependency` | Current factory and requested dependency type. |
| Query | `MissingComponent` | Requested component type. |

Each node in a type path should have a stable type identifier and readable type name. Diagnostics may include registration location, but source location is not part of dependency identity. If a graph has several problems, report the first in registration order and then dependency declaration order so identical inputs yield identical diagnostics. Factory errors retain their source chain rather than converting domain errors to untyped strings.

## 8. Relationship to qubit-reflect

This section refers to local `rs-reflect` revision `6a30402`. Qubit's existing [`qubit-reflect`](https://github.com/qubit-ltd/rs-reflect) provides `#[derive(Reflect)]`, `TypeDescriptor::of::<T>()`, field and method descriptors, checked dynamic construction, and reflection registries built through linker sections or explicit snapshots. It stores **type and operation metadata**, not application component instances. Reflection has explicit local and thread-safety boundaries; a descriptor alone does not prove that a value satisfies IoC's `Send + Sync` requirements.

This makes reflection suitable as an **optional metadata and tooling capability**:

1. If a component implements `Reflect`, an application can explicitly associate its `TypeDescriptor` with an IoC binding for richer type, field, and source diagnostics. Ordinary types and trait objects without `Reflect` remain registerable and fall back to `type_name::<T>()`.
2. Development tools can associate the IoC dependency graph with reflection descriptors to display component structure and dependencies. The caller should pass a reflection registry snapshot explicitly; IoC construction does not implicitly call global `ReflectRegistry::initialize()`.
3. A separate proc-macro crate could provide macros such as `#[Component]`. At compile time, a macro would generate `Dependency::of::<T>()` and ordinary builder registration calls from annotated fields or factory parameters. Generated bindings would still use the validation and construction rules in sections 4–7. If a type also derives `Reflect`, the metadata systems can be associated by exact type identity, but the macro must not assume reflection has automatically registered the component.

Neither the kernel nor macros require components to implement `Reflect`, and the kernel does not directly depend on `qubit-reflect`. This avoids making the not-yet-published reflection crate a release or version prerequisite for the IoC kernel. If diagnostics or tooling later justify it, an optional integration feature, disabled by default, can be added with dependency direction `qubit-ioc → qubit-reflect`. A proc macro needing the IoC runtime should use a narrow IoC-owned interface and versioned generation protocol; it should not reuse `qubit-reflect`'s private `codegen_v3` protocol.

The reflection library's dynamic `construct_struct` requires input to satisfy its checked-construction rules. It cannot replace factories for arbitrary Rust types or automatically convert a concrete type to `dyn Trait`. An IoC dependency graph needs explicit construction semantics, so it does not infer dependencies from reflected fields or automatically register types discovered by reflection. This also avoids exposing fields that should remain private just to support IoC.

## 9. Boundary for static discovery and proc macros

The proposed first application-facing version includes an optional `inventory` static-discovery entry point. The application still decides which crates are linked, and all items go through the same `ContainerBuilder` path for conflict checking, dependency validation, and construction. Inventory iteration order cannot become construction order; submissions need comparable stable source information and must be sorted before entering the builder.

Macros such as `#[Component]` generate factories and dependency lists equivalent to explicit registration. They cannot establish a second container semantics or bypass graph validation. Generated default names are for diagnostics only; component identity remains type plus optional binding ID. A macro may be used on the same type as a `qubit-reflect` macro, but each generates an independent contract and must not require changes to the reflection library's core registry. Conditional bindings, prototype scope, and runtime replacement require separate requirements and compatibility design.

## 10. Suggested implementation boundaries

The implementation should use modules rather than making one container type responsible for everything:

| Module | Main responsibility |
| --- | --- |
| `key` | Unified representation of `TypeId`, optional binding ID, and readable type name. |
| `binding` | Type erasure for instances, sync factories, async factories, and registration order. |
| `graph` | Missing-dependency checks, cycle detection, deterministic topological sorting. |
| `builder` | Registration API, build stages, and factory scheduling. |
| `context` | Restricting factory reads to declared dependencies. |
| `application_context` | Immutable `ApplicationContext` and query API. |
| `error` | Registration, graph-validation, construction, and query errors. |
| `reflect` (unimplemented proposal) | An idea to associate explicit reflection descriptors with bindings; there is no corresponding Cargo feature or runtime behavior today. |

`graph` does not run user factories; `binding` does not determine dependency order; `application_context` no longer holds factories. These boundaries allow graph algorithms and type erasure to be checked independently, while future macros and static discovery only need to call the builder.

## 11. Acceptance scenarios

After implementation, the public API should be able to verify at least these scenarios:

1. Register a configuration, repository, and service with three dependency levels; build successfully and return the same `Arc` instance from repeated `get` calls.
2. Register factories in reverse dependency order and still construct them topologically.
3. Detect missing dependencies and cycles before calling any factory, with readable complete paths in errors.
4. Return the corresponding structured errors for duplicate bindings, duplicate dependencies, and undeclared dependency access.
5. When synchronous `build()` encounters an async factory, call no factories; allow mixed construction through `build_async()`.
6. Preserve the source of async factory failures; after async build cancellation, do not execute later factories.
7. Register and query `Arc<dyn Trait>` by trait type without requiring `Trait: Clone`.
8. Inject `ProviderRegistry<S>` as a component; when a service it creates fails, preserve SPI's original error chain and do not perform another provider fallback.
9. Register, build, and query ordinary types and trait objects without `Reflect`; future reflection integration must not make core construction depend on reflection-registry initialization.
10. Apply the stated rules to multiple IDs for one interface, a unique primary candidate, ambiguity without a primary, and interface aliases sharing the underlying instance with concrete-component queries.

Examples in this document were intended to become compilable integration tests or doctests during implementation. At the time this draft was written, the repository contained only a skeleton. This document is a design baseline and does not mean these scenarios have passed runtime verification.
